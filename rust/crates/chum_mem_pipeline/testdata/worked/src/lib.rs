use std::collections::HashMap;
use std::io::{self, Read};

/// A parsed document ready for indexing.
/// WHY: We keep raw bytes alongside the parsed tree so downstream
/// stages can re-slice without re-reading the file.
pub struct Document {
    pub path: String,
    pub raw: Vec<u8>,
    pub symbols: Vec<Symbol>,
}

pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
}

pub enum SymbolKind {
    Function,
    Struct,
    Trait,
}

/// Trait for anything that can extract symbols from source bytes.
pub trait Extractor: Send + Sync {
    fn extract(&self, src: &[u8]) -> Vec<Symbol>;
}

impl Document {
    /// Parse a file into a Document.
    /// NOTE: Caller must ensure `path` is valid UTF-8.
    pub fn from_reader<R: Read>(path: &str, reader: &mut R) -> io::Result<Self> {
        let mut raw = Vec::new();
        reader.read_to_end(&mut raw)?;
        Ok(Document { path: path.to_owned(), raw, symbols: Vec::new() })
    }

    pub fn index_symbols(&mut self, extractor: &dyn Extractor) {
        self.symbols = extractor.extract(&self.raw);
        self.symbols.sort_by_key(|s| s.line);
    }

    pub fn symbol_map(&self) -> HashMap<&str, &Symbol> {
        self.symbols.iter().map(|s| (s.name.as_str(), s)).collect()
    }
}
