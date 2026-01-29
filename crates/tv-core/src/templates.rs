//! Structure templates for binary data interpretation.
//!
//! Allows defining C-struct-like templates that can be applied
//! to binary data at any offset to interpret the bytes as structured data.

use std::collections::HashMap;
use serde::{Serialize, Deserialize};

/// Primitive data types supported in templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PrimitiveType {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
}

impl PrimitiveType {
    pub fn size(&self) -> usize {
        match self {
            PrimitiveType::U8 | PrimitiveType::I8 => 1,
            PrimitiveType::U16 | PrimitiveType::I16 => 2,
            PrimitiveType::U32 | PrimitiveType::I32 | PrimitiveType::F32 => 4,
            PrimitiveType::U64 | PrimitiveType::I64 | PrimitiveType::F64 => 8,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            PrimitiveType::U8 => "u8",
            PrimitiveType::U16 => "u16",
            PrimitiveType::U32 => "u32",
            PrimitiveType::U64 => "u64",
            PrimitiveType::I8 => "i8",
            PrimitiveType::I16 => "i16",
            PrimitiveType::I32 => "i32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::F32 => "f32",
            PrimitiveType::F64 => "f64",
        }
    }
}

/// Field type in a template.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum FieldType {
    /// Primitive type (u8, u32, etc.)
    Primitive(PrimitiveType),
    /// Fixed-size byte array.
    ByteArray(usize),
    /// Fixed-size string (ASCII).
    String(usize),
    /// Null-terminated string (up to max length).
    CString(usize),
    /// Magic bytes with expected value.
    Magic(Vec<u8>),
    /// Enum with named values.
    Enum {
        base: PrimitiveType,
        values: HashMap<u64, String>,
    },
    /// Flags/bitmask with named bits.
    Flags {
        base: PrimitiveType,
        bits: HashMap<u64, String>,
    },
}

impl FieldType {
    pub fn size(&self) -> usize {
        match self {
            FieldType::Primitive(p) => p.size(),
            FieldType::ByteArray(n) => *n,
            FieldType::String(n) => *n,
            FieldType::CString(n) => *n,
            FieldType::Magic(bytes) => bytes.len(),
            FieldType::Enum { base, .. } => base.size(),
            FieldType::Flags { base, .. } => base.size(),
        }
    }
}

/// A field in a structure template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateField {
    /// Field name.
    pub name: String,
    /// Field type.
    pub field_type: FieldType,
    /// Offset within the structure (computed).
    pub offset: usize,
    /// Optional description/comment.
    pub description: Option<String>,
}

/// A structure template definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StructTemplate {
    /// Template name (e.g., "ELF64 Header").
    pub name: String,
    /// Template description.
    pub description: String,
    /// Fields in order.
    pub fields: Vec<TemplateField>,
    /// Total size of the structure.
    pub size: usize,
    /// Whether the structure uses little-endian byte order.
    pub little_endian: bool,
}

impl StructTemplate {
    /// Create a new template builder.
    pub fn builder(name: &str) -> TemplateBuilder {
        TemplateBuilder::new(name)
    }

    /// Get field by name.
    pub fn field(&self, name: &str) -> Option<&TemplateField> {
        self.fields.iter().find(|f| f.name == name)
    }
}

/// Builder for creating structure templates.
pub struct TemplateBuilder {
    name: String,
    description: String,
    fields: Vec<TemplateField>,
    offset: usize,
    little_endian: bool,
}

impl TemplateBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            description: String::new(),
            fields: Vec::new(),
            offset: 0,
            little_endian: true,
        }
    }

    pub fn description(mut self, desc: &str) -> Self {
        self.description = desc.to_string();
        self
    }

    pub fn little_endian(mut self, le: bool) -> Self {
        self.little_endian = le;
        self
    }

    pub fn field(mut self, name: &str, field_type: FieldType) -> Self {
        let offset = self.offset;
        let size = field_type.size();
        self.fields.push(TemplateField {
            name: name.to_string(),
            field_type,
            offset,
            description: None,
        });
        self.offset += size;
        self
    }

    pub fn field_desc(mut self, name: &str, field_type: FieldType, desc: &str) -> Self {
        let offset = self.offset;
        let size = field_type.size();
        self.fields.push(TemplateField {
            name: name.to_string(),
            field_type,
            offset,
            description: Some(desc.to_string()),
        });
        self.offset += size;
        self
    }

    /// Add padding bytes.
    pub fn padding(mut self, size: usize) -> Self {
        self.offset += size;
        self
    }

    /// Skip to a specific offset.
    pub fn skip_to(mut self, offset: usize) -> Self {
        if offset > self.offset {
            self.offset = offset;
        }
        self
    }

    pub fn build(self) -> StructTemplate {
        StructTemplate {
            name: self.name,
            description: self.description,
            fields: self.fields,
            size: self.offset,
            little_endian: self.little_endian,
        }
    }
}

/// Interpreted value of a field.
#[derive(Debug, Clone)]
pub enum FieldValue {
    /// Unsigned integer.
    Unsigned(u64),
    /// Signed integer.
    Signed(i64),
    /// Floating point.
    Float(f64),
    /// Byte array.
    Bytes(Vec<u8>),
    /// String.
    String(String),
    /// Magic bytes with match status.
    Magic { bytes: Vec<u8>, matches: bool },
    /// Enum value with name.
    Enum { value: u64, name: Option<String> },
    /// Flags with active flag names.
    Flags { value: u64, active: Vec<String> },
    /// Error reading value.
    Error(String),
}

impl FieldValue {
    /// Format the value for display.
    pub fn display(&self) -> String {
        match self {
            FieldValue::Unsigned(v) => {
                if *v > 0xFFFF {
                    format!("0x{:X} ({})", v, v)
                } else if *v > 255 {
                    format!("0x{:X}", v)
                } else {
                    format!("{}", v)
                }
            }
            FieldValue::Signed(v) => format!("{}", v),
            FieldValue::Float(v) => format!("{:.6}", v),
            FieldValue::Bytes(b) => {
                if b.len() <= 8 {
                    b.iter().map(|x| format!("{:02X}", x)).collect::<Vec<_>>().join(" ")
                } else {
                    format!("{} bytes", b.len())
                }
            }
            FieldValue::String(s) => {
                if s.len() > 32 {
                    format!("\"{}...\"", &s[..32])
                } else {
                    format!("\"{}\"", s)
                }
            }
            FieldValue::Magic { bytes, matches } => {
                let hex = bytes.iter().map(|x| format!("{:02X}", x)).collect::<Vec<_>>().join(" ");
                if *matches {
                    format!("{} ✓", hex)
                } else {
                    format!("{} ✗", hex)
                }
            }
            FieldValue::Enum { value, name } => {
                if let Some(n) = name {
                    format!("{} (0x{:X})", n, value)
                } else {
                    format!("0x{:X} (unknown)", value)
                }
            }
            FieldValue::Flags { value, active } => {
                if active.is_empty() {
                    format!("0x{:X}", value)
                } else {
                    format!("0x{:X} [{}]", value, active.join(", "))
                }
            }
            FieldValue::Error(e) => format!("Error: {}", e),
        }
    }

    /// Get the raw numeric value if applicable.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            FieldValue::Unsigned(v) => Some(*v),
            FieldValue::Signed(v) => Some(*v as u64),
            FieldValue::Enum { value, .. } => Some(*value),
            FieldValue::Flags { value, .. } => Some(*value),
            _ => None,
        }
    }
}

/// Result of applying a template to data.
#[derive(Debug, Clone)]
pub struct TemplateResult {
    /// Template that was applied.
    pub template_name: String,
    /// Base offset in the file.
    pub base_offset: u64,
    /// Interpreted field values.
    pub fields: Vec<(TemplateField, FieldValue)>,
    /// Whether all magic bytes matched.
    pub magic_ok: bool,
}

/// Apply a template to data at a given offset.
pub fn apply_template(template: &StructTemplate, data: &[u8], base_offset: u64) -> TemplateResult {
    let mut fields = Vec::new();
    let mut magic_ok = true;
    let le = template.little_endian;

    for field in &template.fields {
        let start = field.offset;
        let size = field.field_type.size();
        let end = start + size;

        let value = if end <= data.len() {
            let bytes = &data[start..end];
            interpret_field(&field.field_type, bytes, le)
        } else {
            FieldValue::Error("Out of bounds".to_string())
        };

        // Check magic
        if let FieldValue::Magic { matches, .. } = &value {
            if !matches {
                magic_ok = false;
            }
        }

        fields.push((field.clone(), value));
    }

    TemplateResult {
        template_name: template.name.clone(),
        base_offset,
        fields,
        magic_ok,
    }
}

/// Interpret bytes according to field type.
fn interpret_field(field_type: &FieldType, bytes: &[u8], little_endian: bool) -> FieldValue {
    match field_type {
        FieldType::Primitive(p) => interpret_primitive(*p, bytes, little_endian),
        FieldType::ByteArray(_) => FieldValue::Bytes(bytes.to_vec()),
        FieldType::String(max_len) => {
            let s: String = bytes.iter()
                .take(*max_len)
                .take_while(|&&b| b != 0)
                .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' })
                .collect();
            FieldValue::String(s)
        }
        FieldType::CString(max_len) => {
            let s: String = bytes.iter()
                .take(*max_len)
                .take_while(|&&b| b != 0)
                .map(|&b| if b.is_ascii_graphic() || b == b' ' { b as char } else { '.' })
                .collect();
            FieldValue::String(s)
        }
        FieldType::Magic(expected) => {
            let matches = bytes == expected.as_slice();
            FieldValue::Magic {
                bytes: bytes.to_vec(),
                matches,
            }
        }
        FieldType::Enum { base, values } => {
            let value = read_unsigned(*base, bytes, little_endian);
            let name = values.get(&value).cloned();
            FieldValue::Enum { value, name }
        }
        FieldType::Flags { base, bits } => {
            let value = read_unsigned(*base, bytes, little_endian);
            let active: Vec<String> = bits
                .iter()
                .filter(|(&bit, _)| value & bit != 0)
                .map(|(_, name)| name.clone())
                .collect();
            FieldValue::Flags { value, active }
        }
    }
}

fn interpret_primitive(p: PrimitiveType, bytes: &[u8], little_endian: bool) -> FieldValue {
    match p {
        PrimitiveType::U8 => FieldValue::Unsigned(bytes[0] as u64),
        PrimitiveType::I8 => FieldValue::Signed(bytes[0] as i8 as i64),
        PrimitiveType::U16 => {
            let v = if little_endian {
                u16::from_le_bytes([bytes[0], bytes[1]])
            } else {
                u16::from_be_bytes([bytes[0], bytes[1]])
            };
            FieldValue::Unsigned(v as u64)
        }
        PrimitiveType::I16 => {
            let v = if little_endian {
                i16::from_le_bytes([bytes[0], bytes[1]])
            } else {
                i16::from_be_bytes([bytes[0], bytes[1]])
            };
            FieldValue::Signed(v as i64)
        }
        PrimitiveType::U32 => {
            let v = if little_endian {
                u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
            } else {
                u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
            };
            FieldValue::Unsigned(v as u64)
        }
        PrimitiveType::I32 => {
            let v = if little_endian {
                i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
            } else {
                i32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
            };
            FieldValue::Signed(v as i64)
        }
        PrimitiveType::U64 => {
            let v = if little_endian {
                u64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])
            } else {
                u64::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])
            };
            FieldValue::Unsigned(v)
        }
        PrimitiveType::I64 => {
            let v = if little_endian {
                i64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])
            } else {
                i64::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])
            };
            FieldValue::Signed(v)
        }
        PrimitiveType::F32 => {
            let v = if little_endian {
                f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
            } else {
                f32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
            };
            FieldValue::Float(v as f64)
        }
        PrimitiveType::F64 => {
            let v = if little_endian {
                f64::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])
            } else {
                f64::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7]])
            };
            FieldValue::Float(v)
        }
    }
}

fn read_unsigned(p: PrimitiveType, bytes: &[u8], little_endian: bool) -> u64 {
    match interpret_primitive(p, bytes, little_endian) {
        FieldValue::Unsigned(v) => v,
        FieldValue::Signed(v) => v as u64,
        _ => 0,
    }
}

// =============================================================================
// JSON Template I/O
// =============================================================================

/// Load a template from a JSON file.
pub fn load_template_from_file(path: &std::path::Path) -> Result<StructTemplate, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;
    load_template_from_json(&content)
}

/// Load a template from a JSON string.
pub fn load_template_from_json(json: &str) -> Result<StructTemplate, String> {
    let template: StructTemplate = serde_json::from_str(json)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    // Validate and recompute offsets
    validate_and_fix_template(template)
}

/// Save a template to a JSON file.
pub fn save_template_to_file(template: &StructTemplate, path: &std::path::Path) -> Result<(), String> {
    let json = serde_json::to_string_pretty(template)
        .map_err(|e| format!("Failed to serialize template: {}", e))?;
    std::fs::write(path, json)
        .map_err(|e| format!("Failed to write file: {}", e))
}

/// Save a template to a JSON string.
pub fn save_template_to_json(template: &StructTemplate) -> Result<String, String> {
    serde_json::to_string_pretty(template)
        .map_err(|e| format!("Failed to serialize template: {}", e))
}

/// Validate and fix offsets in a template (for JSON-loaded templates where offsets may be missing).
fn validate_and_fix_template(mut template: StructTemplate) -> Result<StructTemplate, String> {
    // Recompute offsets sequentially
    let mut offset = 0;
    for field in &mut template.fields {
        field.offset = offset;
        offset += field.field_type.size();
    }
    template.size = offset;

    Ok(template)
}

/// A collection of templates that can be loaded from a JSON file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateCollection {
    /// Name of the collection.
    pub name: String,
    /// Description of the collection.
    #[serde(default)]
    pub description: String,
    /// Templates in this collection.
    pub templates: Vec<StructTemplate>,
}

/// Load a collection of templates from a JSON file.
pub fn load_template_collection(path: &std::path::Path) -> Result<TemplateCollection, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    let mut collection: TemplateCollection = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse JSON: {}", e))?;

    // Validate and fix each template
    for template in &mut collection.templates {
        let mut offset = 0;
        for field in &mut template.fields {
            field.offset = offset;
            offset += field.field_type.size();
        }
        template.size = offset;
    }

    Ok(collection)
}

/// Create an example template JSON for documentation.
pub fn example_template_json() -> String {
    let example = StructTemplate {
        name: "Example Header".to_string(),
        description: "An example structure template".to_string(),
        fields: vec![
            TemplateField {
                name: "magic".to_string(),
                field_type: FieldType::Magic(vec![0x7F, 0x45, 0x4C, 0x46]),
                offset: 0,
                description: Some("Magic bytes identifying the format".to_string()),
            },
            TemplateField {
                name: "version".to_string(),
                field_type: FieldType::Primitive(PrimitiveType::U16),
                offset: 4,
                description: Some("Version number".to_string()),
            },
            TemplateField {
                name: "flags".to_string(),
                field_type: FieldType::Flags {
                    base: PrimitiveType::U32,
                    bits: {
                        let mut m = HashMap::new();
                        m.insert(1, "FLAG_A".to_string());
                        m.insert(2, "FLAG_B".to_string());
                        m.insert(4, "FLAG_C".to_string());
                        m
                    },
                },
                offset: 6,
                description: Some("Option flags".to_string()),
            },
            TemplateField {
                name: "name".to_string(),
                field_type: FieldType::String(16),
                offset: 10,
                description: Some("Name string (16 chars)".to_string()),
            },
        ],
        size: 26,
        little_endian: true,
    };

    serde_json::to_string_pretty(&example).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_roundtrip() {
        let template = StructTemplate::builder("Test JSON")
            .description("Testing JSON serialization")
            .field("magic", FieldType::Magic(vec![0x7F, 0x45, 0x4C, 0x46]))
            .field("version", FieldType::Primitive(PrimitiveType::U16))
            .field("name", FieldType::String(16))
            .build();

        // Serialize to JSON
        let json = save_template_to_json(&template).expect("serialize");
        println!("Serialized JSON:\n{}", json);

        // Parse it back
        let parsed = load_template_from_json(&json).expect("parse");
        assert_eq!(parsed.name, "Test JSON");
        assert_eq!(parsed.fields.len(), 3);
        assert_eq!(parsed.size, 22); // 4 + 2 + 16
    }

    #[test]
    fn test_example_json() {
        let json = example_template_json();
        let template = load_template_from_json(&json).expect("parse example");
        assert_eq!(template.name, "Example Header");
        assert!(!template.fields.is_empty());
    }

    #[test]
    fn test_load_sample_template() {
        // Test parsing the sample template JSON format
        let json = r#"{
            "name": "Test Sample",
            "description": "Test",
            "little_endian": true,
            "fields": [
                {
                    "name": "magic",
                    "field_type": {"type": "magic", "value": [84, 86]},
                    "offset": 0,
                    "description": null
                },
                {
                    "name": "version",
                    "field_type": {"type": "primitive", "value": "u16"},
                    "offset": 2,
                    "description": "Version"
                },
                {
                    "name": "flags",
                    "field_type": {
                        "type": "flags",
                        "value": {"base": "u16", "bits": {"1": "A", "2": "B"}}
                    },
                    "offset": 4,
                    "description": null
                },
                {
                    "name": "kind",
                    "field_type": {
                        "type": "enum",
                        "value": {"base": "u8", "values": {"0": "NONE", "1": "DATA"}}
                    },
                    "offset": 6,
                    "description": null
                }
            ],
            "size": 7
        }"#;

        let template = load_template_from_json(json).expect("parse");
        assert_eq!(template.name, "Test Sample");
        assert_eq!(template.fields.len(), 4);

        // Check magic field
        if let FieldType::Magic(bytes) = &template.fields[0].field_type {
            assert_eq!(bytes, &[84, 86]);
        } else {
            panic!("Expected Magic field type");
        }

        // Check flags field
        if let FieldType::Flags { base, bits } = &template.fields[2].field_type {
            assert_eq!(*base, PrimitiveType::U16);
            assert_eq!(bits.get(&1), Some(&"A".to_string()));
        } else {
            panic!("Expected Flags field type");
        }

        // Check enum field
        if let FieldType::Enum { base, values } = &template.fields[3].field_type {
            assert_eq!(*base, PrimitiveType::U8);
            assert_eq!(values.get(&1), Some(&"DATA".to_string()));
        } else {
            panic!("Expected Enum field type");
        }
    }

    #[test]
    fn test_primitive_sizes() {
        assert_eq!(PrimitiveType::U8.size(), 1);
        assert_eq!(PrimitiveType::U16.size(), 2);
        assert_eq!(PrimitiveType::U32.size(), 4);
        assert_eq!(PrimitiveType::U64.size(), 8);
    }

    #[test]
    fn test_simple_template() {
        let template = StructTemplate::builder("Test")
            .field("magic", FieldType::Magic(vec![0x7F, 0x45, 0x4C, 0x46]))
            .field("class", FieldType::Primitive(PrimitiveType::U8))
            .field("endian", FieldType::Primitive(PrimitiveType::U8))
            .build();

        assert_eq!(template.size, 6);
        assert_eq!(template.fields.len(), 3);
        assert_eq!(template.fields[0].offset, 0);
        assert_eq!(template.fields[1].offset, 4);
        assert_eq!(template.fields[2].offset, 5);
    }

    #[test]
    fn test_apply_template() {
        let template = StructTemplate::builder("Test")
            .field("value16", FieldType::Primitive(PrimitiveType::U16))
            .field("value32", FieldType::Primitive(PrimitiveType::U32))
            .build();

        // Little-endian: 0x0102 = 258, 0x03040506 = 50595078
        let data = [0x02, 0x01, 0x06, 0x05, 0x04, 0x03];
        let result = apply_template(&template, &data, 0);

        assert_eq!(result.fields.len(), 2);

        if let FieldValue::Unsigned(v) = &result.fields[0].1 {
            assert_eq!(*v, 258);
        } else {
            panic!("Expected Unsigned");
        }

        if let FieldValue::Unsigned(v) = &result.fields[1].1 {
            assert_eq!(*v, 0x03040506);
        } else {
            panic!("Expected Unsigned");
        }
    }
}
