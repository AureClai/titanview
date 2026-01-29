//! Syntax highlighting for Rhai scripts.

use egui::text::LayoutJob;
use egui::{Color32, FontId, TextFormat};

/// Token types for syntax highlighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenType {
    Keyword,
    BuiltinFunc,
    String,
    Number,
    Comment,
    Operator,
    Bracket,
    Identifier,
    Normal,
}

impl TokenType {
    fn color(self) -> Color32 {
        match self {
            TokenType::Keyword => Color32::from_rgb(198, 120, 221),    // Purple
            TokenType::BuiltinFunc => Color32::from_rgb(97, 175, 239), // Blue
            TokenType::String => Color32::from_rgb(152, 195, 121),     // Green
            TokenType::Number => Color32::from_rgb(209, 154, 102),     // Orange
            TokenType::Comment => Color32::from_rgb(92, 99, 112),      // Gray
            TokenType::Operator => Color32::from_rgb(86, 182, 194),    // Cyan
            TokenType::Bracket => Color32::from_rgb(224, 108, 117),    // Red
            TokenType::Identifier => Color32::from_rgb(229, 192, 123), // Yellow
            TokenType::Normal => Color32::from_rgb(171, 178, 191),     // Light gray
        }
    }
}

/// Rhai keywords.
const KEYWORDS: &[&str] = &[
    "let", "const", "if", "else", "while", "loop", "for", "in", "break", "continue",
    "return", "throw", "try", "catch", "fn", "private", "import", "export", "as",
    "true", "false", "null", "this", "switch", "case", "default", "do", "until",
];

/// Built-in functions (Rhai + our custom ones).
const BUILTIN_FUNCS: &[&str] = &[
    // Our custom functions
    "file_len", "read_byte", "read_bytes", "write_byte", "viewport", "goto",
    "search", "search_string", "hex", "hex2", "to_hex", "to_hex_byte", "chr",
    "concat", "str",
    // Rhai built-ins
    "print", "debug", "type_of", "is_def_fn", "is_def_var",
    "len", "range", "push", "pop", "shift", "insert", "remove", "clear",
    "contains", "index_of", "filter", "map", "reduce", "for_each", "some", "all",
    "sort", "reverse", "splice", "extract", "split", "drain",
    "min", "max", "abs", "sign", "floor", "ceiling", "round", "int", "float",
    "sin", "cos", "tan", "asin", "acos", "atan", "sinh", "cosh", "tanh",
    "sqrt", "exp", "ln", "log", "log10", "log2", "pow",
    "to_string", "to_int", "to_float", "to_char", "to_array",
    "pad", "trim", "to_upper", "to_lower", "sub_string", "crop", "replace",
    "chars", "bytes", "keys", "values",
];

/// Create a syntax-highlighted layout job for Rhai code.
pub fn highlight_rhai(code: &str, font_id: FontId) -> LayoutJob {
    let mut job = LayoutJob::default();

    let chars: Vec<char> = code.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        let ch = chars[i];

        // Line comment
        if ch == '/' && i + 1 < len && chars[i + 1] == '/' {
            let start = i;
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            add_token(&mut job, &code[byte_index(code, start)..byte_index(code, i)], TokenType::Comment, &font_id);
            continue;
        }

        // Block comment
        if ch == '/' && i + 1 < len && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < len {
                i += 2;
            }
            add_token(&mut job, &code[byte_index(code, start)..byte_index(code, i)], TokenType::Comment, &font_id);
            continue;
        }

        // String with double quotes
        if ch == '"' {
            let start = i;
            i += 1;
            while i < len && chars[i] != '"' {
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                }
                i += 1;
            }
            if i < len {
                i += 1;
            }
            add_token(&mut job, &code[byte_index(code, start)..byte_index(code, i)], TokenType::String, &font_id);
            continue;
        }

        // String with single quotes (character)
        if ch == '\'' {
            let start = i;
            i += 1;
            while i < len && chars[i] != '\'' {
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                }
                i += 1;
            }
            if i < len {
                i += 1;
            }
            add_token(&mut job, &code[byte_index(code, start)..byte_index(code, i)], TokenType::String, &font_id);
            continue;
        }

        // Template string with backticks
        if ch == '`' {
            let start = i;
            i += 1;
            while i < len && chars[i] != '`' {
                if chars[i] == '\\' && i + 1 < len {
                    i += 1;
                }
                i += 1;
            }
            if i < len {
                i += 1;
            }
            add_token(&mut job, &code[byte_index(code, start)..byte_index(code, i)], TokenType::String, &font_id);
            continue;
        }

        // Number (including hex)
        if ch.is_ascii_digit() || (ch == '0' && i + 1 < len && (chars[i + 1] == 'x' || chars[i + 1] == 'X')) {
            let start = i;
            if ch == '0' && i + 1 < len && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
                i += 2;
                while i < len && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
            } else {
                while i < len && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '_') {
                    i += 1;
                }
                // Handle exponent
                if i < len && (chars[i] == 'e' || chars[i] == 'E') {
                    i += 1;
                    if i < len && (chars[i] == '+' || chars[i] == '-') {
                        i += 1;
                    }
                    while i < len && chars[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            add_token(&mut job, &code[byte_index(code, start)..byte_index(code, i)], TokenType::Number, &font_id);
            continue;
        }

        // Identifier or keyword
        if ch.is_alphabetic() || ch == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word = &code[byte_index(code, start)..byte_index(code, i)];
            let token_type = if KEYWORDS.contains(&word) {
                TokenType::Keyword
            } else if BUILTIN_FUNCS.contains(&word) {
                TokenType::BuiltinFunc
            } else {
                TokenType::Identifier
            };
            add_token(&mut job, word, token_type, &font_id);
            continue;
        }

        // Operators
        if is_operator(ch) {
            let start = i;
            // Handle multi-char operators
            while i < len && is_operator(chars[i]) {
                i += 1;
            }
            add_token(&mut job, &code[byte_index(code, start)..byte_index(code, i)], TokenType::Operator, &font_id);
            continue;
        }

        // Brackets
        if is_bracket(ch) {
            add_token(&mut job, &code[byte_index(code, i)..byte_index(code, i + 1)], TokenType::Bracket, &font_id);
            i += 1;
            continue;
        }

        // Whitespace and other characters
        add_token(&mut job, &code[byte_index(code, i)..byte_index(code, i + 1)], TokenType::Normal, &font_id);
        i += 1;
    }

    job
}

/// Add a token to the layout job.
fn add_token(job: &mut LayoutJob, text: &str, token_type: TokenType, font_id: &FontId) {
    job.append(
        text,
        0.0,
        TextFormat {
            font_id: font_id.clone(),
            color: token_type.color(),
            ..Default::default()
        },
    );
}

/// Convert character index to byte index in a string.
fn byte_index(s: &str, char_index: usize) -> usize {
    s.char_indices()
        .nth(char_index)
        .map(|(i, _)| i)
        .unwrap_or(s.len())
}

/// Check if a character is an operator.
fn is_operator(ch: char) -> bool {
    matches!(ch, '+' | '-' | '*' | '/' | '%' | '=' | '!' | '<' | '>' | '&' | '|' | '^' | '~' | '?' | ':' | '.' | ',')
}

/// Check if a character is a bracket.
fn is_bracket(ch: char) -> bool {
    matches!(ch, '(' | ')' | '[' | ']' | '{' | '}')
}
