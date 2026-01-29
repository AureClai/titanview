pub mod types;
pub mod mapped_file;
pub mod entropy;
pub mod pattern;
pub mod classify;
pub mod signatures;
pub mod disasm;
pub mod cfg;
pub mod templates;
pub mod builtin_templates;
pub mod histogram;
pub mod xrefs;
pub mod project;

pub use types::*;
pub use mapped_file::MappedFile;
pub use pattern::{scan_pattern_cpu, scan_pattern_parallel};
pub use disasm::{Architecture, Instruction, DisassemblyResult, disassemble, detect_architecture};
pub use signatures::{CarveInfo, analyze_carve_size, get_extension};
pub use cfg::{ControlFlowGraph, BasicBlock, CfgInstruction, CfgEdge, EdgeType};
pub use templates::{
    StructTemplate, TemplateField, FieldType, FieldValue, TemplateResult, PrimitiveType,
    apply_template, load_template_from_file, load_template_from_json,
    save_template_to_file, save_template_to_json, example_template_json,
    TemplateCollection, load_template_collection,
};
pub use builtin_templates::{builtin_templates, get_template};
pub use histogram::{ByteHistogram, HistogramStats};
pub use xrefs::{XRefTable, XRef, XRefType};
pub use project::{Project, Bookmark, Label, LabelType, Comment, ProjectError};
