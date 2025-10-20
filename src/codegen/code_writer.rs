//! Code writer utilities
//!
//! Provides simple helpers for building code strings with proper indentation
//! and formatting to reduce string concatenation noise.

/// A simple code writer that helps build code with proper indentation
pub struct CodeWriter {
    buffer: String,
    indent_level: usize,
    indent_str: String,
}

impl CodeWriter {
    /// Create a new code writer with default indentation (2 spaces)
    pub fn new() -> Self {
        Self::with_indent("  ")
    }
    
    /// Create a new code writer with custom indentation string
    pub fn with_indent(indent: &str) -> Self {
        CodeWriter {
            buffer: String::new(),
            indent_level: 0,
            indent_str: indent.to_string(),
        }
    }
    
    /// Write a line with current indentation
    pub fn writeln(&mut self, line: &str) {
        self.write_indent();
        self.buffer.push_str(line);
        self.buffer.push('\n');
    }
    
    /// Write multiple lines
    pub fn writeln_multi(&mut self, lines: &[&str]) {
        for line in lines {
            self.writeln(line);
        }
    }
    
    /// Write text without a newline
    pub fn write(&mut self, text: &str) {
        self.buffer.push_str(text);
    }
    
    /// Write current indentation
    pub fn write_indent(&mut self) {
        for _ in 0..self.indent_level {
            self.buffer.push_str(&self.indent_str);
        }
    }
    
    /// Write a blank line
    pub fn blank_line(&mut self) {
        self.buffer.push('\n');
    }
    
    /// Increase indentation level
    pub fn indent(&mut self) {
        self.indent_level += 1;
    }
    
    /// Decrease indentation level
    pub fn dedent(&mut self) {
        if self.indent_level > 0 {
            self.indent_level -= 1;
        }
    }
    
    /// Execute a closure with increased indentation
    pub fn indented<F>(&mut self, f: F) 
    where
        F: FnOnce(&mut Self),
    {
        self.indent();
        f(self);
        self.dedent();
    }
    
    /// Write a block with automatic indentation
    /// Example: writer.block("export class Foo {", "}", |w| { ... })
    pub fn block<F>(&mut self, start: &str, end: &str, f: F)
    where
        F: FnOnce(&mut Self),
    {
        self.writeln(start);
        self.indented(f);
        self.writeln(end);
    }
    
    /// Get the current buffer content
    pub fn to_string(&self) -> String {
        self.buffer.clone()
    }
    
    /// Consume the writer and return the buffer
    pub fn into_string(self) -> String {
        self.buffer
    }
}

impl Default for CodeWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_basic_writing() {
        let mut writer = CodeWriter::new();
        writer.writeln("line 1");
        writer.writeln("line 2");
        
        assert_eq!(writer.to_string(), "line 1\nline 2\n");
    }
    
    #[test]
    fn test_indentation() {
        let mut writer = CodeWriter::new();
        writer.writeln("level 0");
        writer.indent();
        writer.writeln("level 1");
        writer.indent();
        writer.writeln("level 2");
        writer.dedent();
        writer.writeln("level 1");
        writer.dedent();
        writer.writeln("level 0");
        
        assert_eq!(
            writer.to_string(),
            "level 0\n  level 1\n    level 2\n  level 1\nlevel 0\n"
        );
    }
    
    #[test]
    fn test_indented_closure() {
        let mut writer = CodeWriter::new();
        writer.writeln("outer");
        writer.indented(|w| {
            w.writeln("inner 1");
            w.writeln("inner 2");
        });
        writer.writeln("outer");
        
        assert_eq!(
            writer.to_string(),
            "outer\n  inner 1\n  inner 2\nouter\n"
        );
    }
    
    #[test]
    fn test_block() {
        let mut writer = CodeWriter::new();
        writer.block("function foo() {", "}", |w| {
            w.writeln("return 42;");
        });
        
        assert_eq!(
            writer.to_string(),
            "function foo() {\n  return 42;\n}\n"
        );
    }
    
    #[test]
    fn test_nested_blocks() {
        let mut writer = CodeWriter::new();
        writer.block("class Foo {", "}", |w| {
            w.block("constructor() {", "}", |w| {
                w.writeln("this.x = 1;");
            });
        });
        
        assert_eq!(
            writer.to_string(),
            "class Foo {\n  constructor() {\n    this.x = 1;\n  }\n}\n"
        );
    }
    
    #[test]
    fn test_blank_line() {
        let mut writer = CodeWriter::new();
        writer.writeln("line 1");
        writer.blank_line();
        writer.writeln("line 2");
        
        assert_eq!(writer.to_string(), "line 1\n\nline 2\n");
    }
    
    #[test]
    fn test_custom_indent() {
        let mut writer = CodeWriter::with_indent("    ");
        writer.writeln("level 0");
        writer.indent();
        writer.writeln("level 1");
        
        assert_eq!(writer.to_string(), "level 0\n    level 1\n");
    }
}
