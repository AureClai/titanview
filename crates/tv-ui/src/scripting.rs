//! Scripting support for TitanView using Rhai.
//!
//! Provides a script console for automating repetitive tasks like:
//! - XOR operations on byte ranges
//! - Pattern searching
//! - Data transformation
//! - Navigation

use rhai::{Engine, Scope, AST, EvalAltResult, ImmutableString};
use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

/// Maximum lines to keep in output history.
const MAX_OUTPUT_LINES: usize = 1000;

/// Script execution context shared with the Rhai engine.
#[derive(Clone)]
pub struct ScriptContext {
    /// File data (read-only view).
    pub file_data: Arc<Vec<u8>>,
    /// Pending byte edits: (offset, value).
    pub pending_edits: Arc<Mutex<Vec<(u64, u8)>>>,
    /// Output lines from print statements.
    pub output: Arc<Mutex<VecDeque<String>>>,
    /// Current viewport offset.
    pub viewport_offset: u64,
    /// Navigation request: set to Some(offset) to navigate.
    pub goto_request: Arc<Mutex<Option<u64>>>,
    /// Search results from script.
    pub search_results: Arc<Mutex<Vec<u64>>>,
}

impl ScriptContext {
    pub fn new(file_data: Vec<u8>, viewport_offset: u64) -> Self {
        Self {
            file_data: Arc::new(file_data),
            pending_edits: Arc::new(Mutex::new(Vec::new())),
            output: Arc::new(Mutex::new(VecDeque::new())),
            viewport_offset,
            goto_request: Arc::new(Mutex::new(None)),
            search_results: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Add a line to the output.
    pub fn print(&self, msg: &str) {
        if let Ok(mut output) = self.output.lock() {
            output.push_back(msg.to_string());
            while output.len() > MAX_OUTPUT_LINES {
                output.pop_front();
            }
        }
    }

    /// Get all output lines.
    pub fn get_output(&self) -> Vec<String> {
        self.output.lock().map(|o| o.iter().cloned().collect()).unwrap_or_default()
    }

    /// Clear output.
    pub fn clear_output(&self) {
        if let Ok(mut output) = self.output.lock() {
            output.clear();
        }
    }

    /// Get pending edits and clear them.
    pub fn take_edits(&self) -> Vec<(u64, u8)> {
        self.pending_edits.lock().map(|mut e| std::mem::take(&mut *e)).unwrap_or_default()
    }

    /// Get goto request and clear it.
    pub fn take_goto(&self) -> Option<u64> {
        self.goto_request.lock().ok().and_then(|mut g| g.take())
    }

    /// Get search results and clear them.
    pub fn take_search_results(&self) -> Vec<u64> {
        self.search_results.lock().map(|mut r| std::mem::take(&mut *r)).unwrap_or_default()
    }
}

/// State for the script console window.
pub struct ScriptState {
    /// Rhai engine.
    engine: Engine,
    /// Current script text.
    pub script_text: String,
    /// Script history (for up/down navigation).
    pub history: Vec<String>,
    /// Current history index.
    pub history_index: Option<usize>,
    /// Output lines.
    pub output: Vec<String>,
    /// Last error message.
    pub last_error: Option<String>,
    /// Whether the console is in "REPL" mode (execute on Enter).
    pub repl_mode: bool,
    /// Compiled AST cache.
    compiled_ast: Option<AST>,
    /// Current context (updated before each run).
    context: Option<ScriptContext>,
    /// Example scripts.
    pub examples: Vec<(&'static str, &'static str)>,
}

impl Default for ScriptState {
    fn default() -> Self {
        let mut state = Self {
            engine: create_engine(),
            script_text: String::new(),
            history: Vec::new(),
            history_index: None,
            output: Vec::new(),
            last_error: None,
            repl_mode: false,
            compiled_ast: None,
            context: None,
            examples: vec![
                ("XOR Range", r#"// XOR bytes from offset 0x100 to 0x200 with key 0x42
for i in range(0x100, 0x200) {
    let b = read_byte(i);
    write_byte(i, b ^ 0x42);
}
print("XOR complete!");"#),
                ("Find Pattern", r#"// Find all occurrences of "MZ" header
let results = search([0x4D, 0x5A]);
print(`Found ${results.len()} MZ headers`);
for offset in results {
    print(`  ${hex(offset)}`);
}"#),
                ("Entropy Scan", r#"// Calculate entropy of first 256 bytes
let counts = [];
for i in range(0, 256) { counts.push(0); }
for i in range(0, min(256, file_len())) {
    let b = read_byte(i);
    counts[b] += 1;
}
let entropy = 0.0;
for c in counts {
    if c > 0 {
        let p = c / 256.0;
        entropy -= p * log2(p);
    }
}
print(`Entropy: ${entropy} bits`);"#),
                ("Hex Dump", r#"// Hex dump of current viewport (16 bytes)
let off = viewport();
print(`Offset: ${hex(off)}`);
let hex_line = "";
let ascii_line = "";
for i in range(0, 16) {
    let b = read_byte(off + i);
    hex_line += hex2(b) + " ";
    if b >= 0x20 && b < 0x7F {
        ascii_line += chr(b);
    } else {
        ascii_line += ".";
    }
}
print(hex_line);
print(`|${ascii_line}|`);"#),
                ("Statistics", r#"// Byte frequency analysis
let zeros = 0;
let printable = 0;
let high = 0;
let total = min(file_len(), 10000); // Sample first 10KB
for i in range(0, total) {
    let b = read_byte(i);
    if b == 0 { zeros += 1; }
    if b >= 0x20 && b < 0x7F { printable += 1; }
    if b >= 0x80 { high += 1; }
}
let zp = zeros * 100 / total;
let pp = printable * 100 / total;
let hp = high * 100 / total;
print(`Sample: ${total} bytes`);
print(`  Zeros: ${zeros} (${zp}%)`);
print(`  Printable: ${printable} (${pp}%)`);
print(`  High bytes: ${high} (${hp}%)`);"#),
            ],
        };
        state.output.push("TitanView Script Console".to_string());
        state.output.push("Type 'help' for available functions.".to_string());
        state.output.push(String::new());
        state
    }
}

impl ScriptState {
    /// Create a new script state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the context with current file data.
    pub fn set_context(&mut self, file_data: Vec<u8>, viewport_offset: u64) {
        self.context = Some(ScriptContext::new(file_data, viewport_offset));
    }

    /// Execute the current script.
    pub fn execute(&mut self) -> Result<(), String> {
        let script = self.script_text.trim();
        if script.is_empty() {
            return Ok(());
        }

        // Add to history
        if self.history.last().map(|s| s.as_str()) != Some(script) {
            self.history.push(script.to_string());
        }
        self.history_index = None;

        // Handle built-in commands
        if script == "help" {
            self.print_help();
            return Ok(());
        }
        if script == "clear" {
            self.output.clear();
            return Ok(());
        }
        if script == "examples" {
            self.print_examples();
            return Ok(());
        }

        // Get or create context
        let ctx = match &self.context {
            Some(c) => c.clone(),
            None => {
                self.output.push("Error: No file loaded".to_string());
                return Err("No file loaded".to_string());
            }
        };

        // Register context functions
        let engine = create_engine_with_context(ctx.clone());

        // Log the script being executed
        self.output.push(format!("> {}", script.lines().next().unwrap_or("")));
        if script.lines().count() > 1 {
            self.output.push("  ...".to_string());
        }

        // Execute
        let mut scope = Scope::new();
        match engine.eval_with_scope::<rhai::Dynamic>(&mut scope, script) {
            Ok(result) => {
                // Collect output from context
                for line in ctx.get_output() {
                    self.output.push(line);
                }

                // Show result if not unit
                if !result.is_unit() {
                    self.output.push(format!("=> {}", result));
                }

                self.last_error = None;
                Ok(())
            }
            Err(e) => {
                let err_msg = format_error(&e);
                self.output.push(format!("Error: {}", err_msg));
                self.last_error = Some(err_msg.clone());
                Err(err_msg)
            }
        }
    }

    /// Get pending edits from the last script execution.
    pub fn take_edits(&mut self) -> Vec<(u64, u8)> {
        self.context.as_ref().map(|c| c.take_edits()).unwrap_or_default()
    }

    /// Get goto request from the last script execution.
    pub fn take_goto(&mut self) -> Option<u64> {
        self.context.as_ref().and_then(|c| c.take_goto())
    }

    /// Get search results from the last script execution.
    pub fn take_search_results(&mut self) -> Vec<u64> {
        self.context.as_ref().map(|c| c.take_search_results()).unwrap_or_default()
    }

    /// Navigate history up.
    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                self.history_index = Some(self.history.len() - 1);
            }
            Some(idx) if idx > 0 => {
                self.history_index = Some(idx - 1);
            }
            _ => {}
        }
        if let Some(idx) = self.history_index {
            if let Some(script) = self.history.get(idx) {
                self.script_text = script.clone();
            }
        }
    }

    /// Navigate history down.
    pub fn history_down(&mut self) {
        match self.history_index {
            Some(idx) if idx < self.history.len() - 1 => {
                self.history_index = Some(idx + 1);
                if let Some(script) = self.history.get(idx + 1) {
                    self.script_text = script.clone();
                }
            }
            Some(_) => {
                self.history_index = None;
                self.script_text.clear();
            }
            None => {}
        }
    }

    /// Print help text.
    fn print_help(&mut self) {
        self.output.push("=== TitanView Script API ===".to_string());
        self.output.push(String::new());
        self.output.push("File Operations:".to_string());
        self.output.push("  file_len()              - Get file size in bytes".to_string());
        self.output.push("  read_byte(offset)       - Read byte at offset".to_string());
        self.output.push("  read_bytes(offset, len) - Read multiple bytes as array".to_string());
        self.output.push("  write_byte(offset, val) - Queue byte write (applied on confirm)".to_string());
        self.output.push(String::new());
        self.output.push("Navigation:".to_string());
        self.output.push("  viewport()              - Get current viewport offset".to_string());
        self.output.push("  goto(offset)            - Navigate to offset".to_string());
        self.output.push(String::new());
        self.output.push("Search:".to_string());
        self.output.push("  search([bytes])         - Find all occurrences of byte pattern".to_string());
        self.output.push("  search_string(\"text\")   - Find all occurrences of string".to_string());
        self.output.push(String::new());
        self.output.push("Utilities:".to_string());
        self.output.push("  print(msg)              - Print to console".to_string());
        self.output.push("  `text ${var}`           - String interpolation".to_string());
        self.output.push("  hex(n)                  - Format as 0xHEX".to_string());
        self.output.push("  hex2(n)                 - Format byte as 2-digit hex".to_string());
        self.output.push("  to_hex(n)               - Number to hex (no prefix)".to_string());
        self.output.push("  chr(n)                  - Convert byte to character".to_string());
        self.output.push("  min(a, b), max(a, b)    - Min/max functions".to_string());
        self.output.push("  log2(n)                 - Log base 2".to_string());
        self.output.push(String::new());
        self.output.push("Commands:".to_string());
        self.output.push("  help                    - Show this help".to_string());
        self.output.push("  clear                   - Clear console output".to_string());
        self.output.push("  examples                - Show example scripts".to_string());
    }

    /// Print example scripts.
    fn print_examples(&mut self) {
        self.output.push("=== Example Scripts ===".to_string());
        self.output.push("Click an example in the sidebar to load it.".to_string());
        for (name, _) in &self.examples {
            self.output.push(format!("  - {}", name));
        }
    }

    /// Load an example script.
    pub fn load_example(&mut self, index: usize) {
        if let Some((name, script)) = self.examples.get(index) {
            self.script_text = script.to_string();
            self.output.push(format!("Loaded example: {}", name));
        }
    }
}

/// Create a basic Rhai engine.
fn create_engine() -> Engine {
    let mut engine = Engine::new();
    engine.set_max_expr_depths(64, 64);
    engine.set_max_operations(1_000_000);
    engine
}

/// Create a Rhai engine with context bindings.
fn create_engine_with_context(ctx: ScriptContext) -> Engine {
    let mut engine = Engine::new();
    engine.set_max_expr_depths(64, 64);
    engine.set_max_operations(1_000_000);

    // Capture print output using Rhai's built-in mechanism
    let ctx_print = ctx.clone();
    engine.on_print(move |text| {
        ctx_print.print(text);
    });

    // Also capture debug output
    let ctx_debug = ctx.clone();
    engine.on_debug(move |text, _source, _pos| {
        ctx_debug.print(&format!("[DEBUG] {}", text));
    });

    // File length
    let ctx_clone = ctx.clone();
    engine.register_fn("file_len", move || -> i64 {
        ctx_clone.file_data.len() as i64
    });

    // Read byte
    let ctx_clone = ctx.clone();
    engine.register_fn("read_byte", move |offset: i64| -> i64 {
        if offset < 0 || offset as usize >= ctx_clone.file_data.len() {
            return 0;
        }
        ctx_clone.file_data[offset as usize] as i64
    });

    // Read bytes
    let ctx_clone = ctx.clone();
    engine.register_fn("read_bytes", move |offset: i64, len: i64| -> rhai::Array {
        let start = offset.max(0) as usize;
        let end = (start + len.max(0) as usize).min(ctx_clone.file_data.len());
        ctx_clone.file_data[start..end]
            .iter()
            .map(|&b| rhai::Dynamic::from(b as i64))
            .collect()
    });

    // Write byte
    let ctx_clone = ctx.clone();
    engine.register_fn("write_byte", move |offset: i64, value: i64| {
        if offset >= 0 && (offset as usize) < ctx_clone.file_data.len() {
            if let Ok(mut edits) = ctx_clone.pending_edits.lock() {
                edits.push((offset as u64, (value & 0xFF) as u8));
            }
        }
    });

    // Viewport offset
    let ctx_clone = ctx.clone();
    engine.register_fn("viewport", move || -> i64 {
        ctx_clone.viewport_offset as i64
    });

    // Goto
    let ctx_clone = ctx.clone();
    engine.register_fn("goto", move |offset: i64| {
        if let Ok(mut goto) = ctx_clone.goto_request.lock() {
            *goto = Some(offset.max(0) as u64);
        }
    });

    // Search for bytes
    let ctx_clone = ctx.clone();
    engine.register_fn("search", move |pattern: rhai::Array| -> rhai::Array {
        let pattern: Vec<u8> = pattern.iter()
            .filter_map(|d| d.as_int().ok().map(|i| i as u8))
            .collect();

        if pattern.is_empty() {
            return rhai::Array::new();
        }

        let data = &ctx_clone.file_data;
        let mut results = Vec::new();

        for i in 0..data.len().saturating_sub(pattern.len() - 1) {
            if &data[i..i + pattern.len()] == pattern.as_slice() {
                results.push(rhai::Dynamic::from(i as i64));
            }
        }

        // Store in context for external access
        if let Ok(mut sr) = ctx_clone.search_results.lock() {
            *sr = results.iter()
                .filter_map(|d| d.as_int().ok().map(|i| i as u64))
                .collect();
        }

        results
    });

    // Search for string
    let ctx_clone = ctx.clone();
    engine.register_fn("search_string", move |needle: &str| -> rhai::Array {
        let pattern = needle.as_bytes();
        if pattern.is_empty() {
            return rhai::Array::new();
        }

        let data = &ctx_clone.file_data;
        let mut results = Vec::new();

        for i in 0..data.len().saturating_sub(pattern.len() - 1) {
            if &data[i..i + pattern.len()] == pattern {
                results.push(rhai::Dynamic::from(i as i64));
            }
        }

        results
    });

    // Hex conversion - return ImmutableString for Rhai string concatenation compatibility
    engine.register_fn("to_hex", |n: i64| -> ImmutableString {
        format!("{:X}", n).into()
    });

    engine.register_fn("to_hex_byte", |n: i64| -> ImmutableString {
        format!("{:02X}", n & 0xFF).into()
    });

    // Char conversion
    engine.register_fn("chr", |n: i64| -> ImmutableString {
        let b = (n & 0xFF) as u8;
        if b.is_ascii_graphic() || b == b' ' {
            (b as char).to_string().into()
        } else {
            ".".into()
        }
    });

    // String concatenation - takes an array of values
    engine.register_fn("concat", |arr: rhai::Array| -> ImmutableString {
        arr.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join("")
            .into()
    });

    // Hex format helpers
    engine.register_fn("hex", |n: i64| -> ImmutableString {
        format!("0x{:X}", n).into()
    });

    engine.register_fn("hex2", |n: i64| -> ImmutableString {
        format!("{:02X}", n & 0xFF).into()
    });

    // str() - convert any value to string
    engine.register_fn("str", |v: rhai::Dynamic| -> ImmutableString {
        v.to_string().into()
    });

    engine.register_fn("str", |v: i64| -> ImmutableString {
        v.to_string().into()
    });

    // Math functions
    engine.register_fn("min", |a: i64, b: i64| -> i64 { a.min(b) });
    engine.register_fn("max", |a: i64, b: i64| -> i64 { a.max(b) });
    engine.register_fn("log2", |n: f64| -> f64 { n.log2() });
    engine.register_fn("log2", |n: i64| -> f64 { (n as f64).log2() });

    engine
}

/// Format a Rhai error for display.
fn format_error(err: &EvalAltResult) -> String {
    match err {
        EvalAltResult::ErrorParsing(_, pos) => {
            format!("Parse error at line {}", pos.line().unwrap_or(0))
        }
        EvalAltResult::ErrorRuntime(msg, pos) => {
            if pos.is_none() {
                format!("Runtime error: {}", msg)
            } else {
                format!("Runtime error at line {}: {}", pos.line().unwrap_or(0), msg)
            }
        }
        _ => format!("{}", err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_script_state_default() {
        let state = ScriptState::default();
        assert!(!state.output.is_empty()); // Has welcome message
        assert!(state.history.is_empty());
    }

    #[test]
    fn test_context_basic() {
        let data = vec![0x41, 0x42, 0x43, 0x44]; // "ABCD"
        let ctx = ScriptContext::new(data, 0);

        // Test print
        ctx.print("Hello");
        let output = ctx.get_output();
        assert_eq!(output.len(), 1);
        assert_eq!(output[0], "Hello");
    }

    #[test]
    fn test_engine_read_byte() {
        let data = vec![0x41, 0x42, 0x43, 0x44];
        let ctx = ScriptContext::new(data, 0);
        let engine = create_engine_with_context(ctx);

        let result: i64 = engine.eval("read_byte(0)").unwrap();
        assert_eq!(result, 0x41);

        let result: i64 = engine.eval("read_byte(3)").unwrap();
        assert_eq!(result, 0x44);
    }

    #[test]
    fn test_engine_file_len() {
        let data = vec![0; 1000];
        let ctx = ScriptContext::new(data, 0);
        let engine = create_engine_with_context(ctx);

        let result: i64 = engine.eval("file_len()").unwrap();
        assert_eq!(result, 1000);
    }

    #[test]
    fn test_engine_search() {
        let data = vec![0x41, 0x42, 0x43, 0x41, 0x42, 0x44];
        let ctx = ScriptContext::new(data, 0);
        let engine = create_engine_with_context(ctx);

        let result: rhai::Array = engine.eval("search([0x41, 0x42])").unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_engine_write_byte() {
        let data = vec![0x41, 0x42, 0x43];
        let ctx = ScriptContext::new(data, 0);
        let engine = create_engine_with_context(ctx.clone());

        engine.eval::<()>("write_byte(1, 0xFF)").unwrap();

        let edits = ctx.take_edits();
        assert_eq!(edits.len(), 1);
        assert_eq!(edits[0], (1, 0xFF));
    }
}
