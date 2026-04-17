//! Tree-sitter based AST extraction for 20+ programming languages.
//!
//! Replaces regex-based extraction with proper AST parsing, extracting:
//! - Symbol definitions (functions, classes, structs, traits, interfaces, enums, modules)
//! - Import/dependency edges
//! - Call graph edges (function call sites)
//! - Docstrings and rationale comments

use std::collections::HashSet;

use streaming_iterator::StreamingIterator as _;
use tree_sitter::{Language, Node, Parser, Query, QueryCursor};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A symbol extracted from an AST (function, class, struct, etc.).
#[derive(Debug, Clone)]
pub struct AstSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub line: usize,
    pub doc_comment: Option<String>,
    /// Parent symbol name (for containment edges: method → class).
    pub parent_name: Option<String>,
    /// Return type annotation (e.g. `Result<Vec<Node>>`, `string`).
    pub return_type: Option<String>,
    /// Parameter types (name, type) pairs.
    pub param_types: Vec<(String, String)>,
}

/// The kind of a symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolKind {
    Function,
    Class,
    Struct,
    Trait,
    Interface,
    Enum,
    Module,
    Type,
    Constant,
    Protocol,
    Field,
}

impl SymbolKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Class => "class",
            Self::Struct => "struct",
            Self::Trait => "trait",
            Self::Interface => "interface",
            Self::Enum => "enum",
            Self::Module => "module",
            Self::Type => "type",
            Self::Constant => "const",
            Self::Protocol => "protocol",
            Self::Field => "field",
        }
    }
}

/// An import extracted from an AST.
#[derive(Debug, Clone)]
pub struct AstImport {
    pub source: String,
    pub line: usize,
    pub is_relative: bool,
}

/// A function call site extracted from an AST.
#[derive(Debug, Clone)]
pub struct AstCall {
    pub callee: String,
    pub line: usize,
}

/// A rationale comment (WHY:, NOTE:, IMPORTANT:, etc.).
#[derive(Debug, Clone)]
pub struct AstRationale {
    pub tag: String,
    pub body: String,
    pub line: usize,
}

/// Complete extraction result from a single file.
#[derive(Debug, Clone)]
pub struct AstExtraction {
    pub language: &'static str,
    pub symbols: Vec<AstSymbol>,
    pub imports: Vec<AstImport>,
    pub calls: Vec<AstCall>,
    pub rationales: Vec<AstRationale>,
}

// ---------------------------------------------------------------------------
// Supported languages
// ---------------------------------------------------------------------------

/// Supported language identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lang {
    Python,
    TypeScript,
    Tsx,
    JavaScript,
    Go,
    Rust,
    Java,
    C,
    Cpp,
    Ruby,
    CSharp,
    Kotlin,
    Scala,
    Php,
    Swift,
    Lua,
    Zig,
    Elixir,
    Julia,
}

impl Lang {
    /// Detect language from file extension. Returns `None` for unsupported extensions.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "py" => Some(Self::Python),
            "ts" => Some(Self::TypeScript),
            "tsx" => Some(Self::Tsx),
            "js" | "jsx" | "mjs" | "cjs" => Some(Self::JavaScript),
            "go" => Some(Self::Go),
            "rs" => Some(Self::Rust),
            "java" => Some(Self::Java),
            "c" | "h" => Some(Self::C),
            "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "mm" => Some(Self::Cpp),
            "rb" => Some(Self::Ruby),
            "cs" => Some(Self::CSharp),
            "kt" | "kts" => Some(Self::Kotlin),
            "scala" | "sc" => Some(Self::Scala),
            "php" => Some(Self::Php),
            "swift" => Some(Self::Swift),
            "lua" => Some(Self::Lua),
            "zig" => Some(Self::Zig),
            "ex" | "exs" => Some(Self::Elixir),
            "jl" => Some(Self::Julia),
            "m" => Some(Self::C),   // Objective-C uses C grammar as fallback
            "ps1" | "psm1" => None, // PowerShell not supported via tree-sitter (no stable grammar with compatible API)
            _ => None,
        }
    }

    fn tree_sitter_language(self) -> Language {
        match self {
            Self::Python => tree_sitter_python::LANGUAGE.into(),
            Self::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Self::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Self::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
            Self::Go => tree_sitter_go::LANGUAGE.into(),
            Self::Rust => tree_sitter_rust::LANGUAGE.into(),
            Self::Java => tree_sitter_java::LANGUAGE.into(),
            Self::C => tree_sitter_c::LANGUAGE.into(),
            Self::Cpp => tree_sitter_cpp::LANGUAGE.into(),
            Self::Ruby => tree_sitter_ruby::LANGUAGE.into(),
            Self::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
            Self::Kotlin => tree_sitter_java::LANGUAGE.into(), // Kotlin shares Java grammar as fallback
            Self::Scala => tree_sitter_scala::LANGUAGE.into(),
            Self::Php => tree_sitter_php::LANGUAGE_PHP.into(),
            Self::Swift => tree_sitter_java::LANGUAGE.into(), // Swift fallback to Java-like grammar
            Self::Lua => tree_sitter_lua::LANGUAGE.into(),
            Self::Zig => tree_sitter_zig::LANGUAGE.into(),
            Self::Elixir => tree_sitter_elixir::LANGUAGE.into(),
            Self::Julia => tree_sitter_julia::LANGUAGE.into(),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::Tsx => "tsx",
            Self::JavaScript => "javascript",
            Self::Go => "go",
            Self::Rust => "rust",
            Self::Java => "java",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::Ruby => "ruby",
            Self::CSharp => "csharp",
            Self::Kotlin => "kotlin",
            Self::Scala => "scala",
            Self::Php => "php",
            Self::Swift => "swift",
            Self::Lua => "lua",
            Self::Zig => "zig",
            Self::Elixir => "elixir",
            Self::Julia => "julia",
        }
    }
}

// ---------------------------------------------------------------------------
// Main extraction entry point
// ---------------------------------------------------------------------------

/// Parse a source file and extract symbols, imports, calls, and rationale comments.
///
/// Returns `None` if the file extension is not supported or parsing fails.
pub fn extract_ast(extension: &str, source: &str) -> Option<AstExtraction> {
    let lang = Lang::from_extension(extension)?;
    let ts_lang = lang.tree_sitter_language();

    let mut parser = Parser::new();
    parser.set_language(&ts_lang).ok()?;
    let tree = parser.parse(source, None)?;
    let root = tree.root_node();
    let bytes = source.as_bytes();

    let mut symbols = extract_symbols(lang, &ts_lang, root, bytes);
    let imports = extract_imports(lang, &ts_lang, root, bytes);
    let calls = extract_calls(lang, &ts_lang, root, bytes);
    let rationales = extract_rationales(root, bytes);

    // v2.2.2: Extract arrow functions / const exports (JS/TS)
    if matches!(
        lang,
        Lang::JavaScript | Lang::TypeScript | Lang::Tsx
    ) {
        extract_arrow_functions(root, bytes, &mut symbols);
    }

    // v2.2.2: Populate doc comments + containment (parent_name) via tree walk
    populate_doc_comments_and_containment(lang, root, bytes, &mut symbols);

    // v2.2.2: Extract return types and parameter types from function signatures
    populate_type_signatures(lang, root, bytes, &mut symbols);

    Some(AstExtraction {
        language: lang.name(),
        symbols,
        imports,
        calls,
        rationales,
    })
}

// ---------------------------------------------------------------------------
// Symbol extraction (per-language tree-sitter queries)
// ---------------------------------------------------------------------------

fn extract_symbols(lang: Lang, ts_lang: &Language, root: Node, source: &[u8]) -> Vec<AstSymbol> {
    let query_src = symbol_query_for(lang);
    if query_src.is_empty() {
        return extract_symbols_walk(lang, root, source);
    }
    let Ok(query) = Query::new(ts_lang, query_src) else {
        return extract_symbols_walk(lang, root, source);
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source);
    let names = query.capture_names();

    let mut symbols = Vec::new();
    while let Some(m) = {
        matches.advance();
        matches.get()
    } {
        let mut name = None;
        let mut kind = SymbolKind::Function;
        let mut line = 0;
        for cap in m.captures {
            let cap_name = &names[cap.index as usize];
            let text = cap.node.utf8_text(source).unwrap_or_default();
            match *cap_name {
                "fn_name" => {
                    name = Some(text.to_string());
                    kind = SymbolKind::Function;
                    line = cap.node.start_position().row + 1;
                }
                "class_name" => {
                    name = Some(text.to_string());
                    kind = SymbolKind::Class;
                    line = cap.node.start_position().row + 1;
                }
                "struct_name" => {
                    name = Some(text.to_string());
                    kind = SymbolKind::Struct;
                    line = cap.node.start_position().row + 1;
                }
                "trait_name" => {
                    name = Some(text.to_string());
                    kind = SymbolKind::Trait;
                    line = cap.node.start_position().row + 1;
                }
                "interface_name" => {
                    name = Some(text.to_string());
                    kind = SymbolKind::Interface;
                    line = cap.node.start_position().row + 1;
                }
                "enum_name" => {
                    name = Some(text.to_string());
                    kind = SymbolKind::Enum;
                    line = cap.node.start_position().row + 1;
                }
                "module_name" => {
                    name = Some(text.to_string());
                    kind = SymbolKind::Module;
                    line = cap.node.start_position().row + 1;
                }
                "type_name" => {
                    name = Some(text.to_string());
                    kind = SymbolKind::Type;
                    line = cap.node.start_position().row + 1;
                }
                "const_name" => {
                    name = Some(text.to_string());
                    kind = SymbolKind::Constant;
                    line = cap.node.start_position().row + 1;
                }
                "field_name" => {
                    name = Some(text.to_string());
                    kind = SymbolKind::Field;
                    line = cap.node.start_position().row + 1;
                }
                _ => {}
            }
        }
        if let Some(n) = name {
            if !n.is_empty() {
                symbols.push(AstSymbol {
                    name: n,
                    kind,
                    line,
                    doc_comment: None,
                    parent_name: None,
                    return_type: None,
                    param_types: Vec::new(),
                });
            }
        }
    }
    symbols
}

fn symbol_query_for(lang: Lang) -> &'static str {
    match lang {
        Lang::Python => {
            r#"
            (function_definition name: (identifier) @fn_name)
            (class_definition name: (identifier) @class_name)
            "#
        }
        Lang::TypeScript | Lang::Tsx => {
            r#"
            (function_declaration name: (identifier) @fn_name)
            (class_declaration name: (type_identifier) @class_name)
            (interface_declaration name: (type_identifier) @interface_name)
            (type_alias_declaration name: (type_identifier) @type_name)
            (enum_declaration name: (identifier) @enum_name)
            (method_definition name: (property_identifier) @fn_name)
            (public_field_definition name: (property_identifier) @field_name)
            "#
        }
        Lang::JavaScript => {
            r#"
            (function_declaration name: (identifier) @fn_name)
            (class_declaration name: (identifier) @class_name)
            (method_definition name: (property_identifier) @fn_name)
            "#
        }
        Lang::Go => {
            r#"
            (function_declaration name: (identifier) @fn_name)
            (method_declaration name: (field_identifier) @fn_name)
            (type_declaration (type_spec name: (type_identifier) @struct_name))
            "#
        }
        Lang::Rust => {
            r#"
            (function_item name: (identifier) @fn_name)
            (struct_item name: (type_identifier) @struct_name)
            (enum_item name: (type_identifier) @enum_name)
            (trait_item name: (type_identifier) @trait_name)
            (impl_item trait: (type_identifier) @trait_name)
            (type_item name: (type_identifier) @type_name)
            (const_item name: (identifier) @const_name)
            (field_declaration name: (field_identifier) @field_name)
            "#
        }
        Lang::Java | Lang::Kotlin => {
            r#"
            (method_declaration name: (identifier) @fn_name)
            (class_declaration name: (identifier) @class_name)
            (interface_declaration name: (identifier) @interface_name)
            (enum_declaration name: (identifier) @enum_name)
            "#
        }
        Lang::C => {
            r#"
            (function_definition declarator: (function_declarator declarator: (identifier) @fn_name))
            (struct_specifier name: (type_identifier) @struct_name)
            (enum_specifier name: (type_identifier) @enum_name)
            (type_definition declarator: (type_identifier) @type_name)
            "#
        }
        Lang::Cpp => {
            r#"
            (function_definition declarator: (function_declarator declarator: (identifier) @fn_name))
            (function_definition declarator: (function_declarator declarator: (qualified_identifier) @fn_name))
            (class_specifier name: (type_identifier) @class_name)
            (struct_specifier name: (type_identifier) @struct_name)
            (enum_specifier name: (type_identifier) @enum_name)
            (namespace_definition name: (namespace_identifier) @module_name)
            "#
        }
        Lang::Ruby => {
            r#"
            (method name: (identifier) @fn_name)
            (class name: (constant) @class_name)
            (module name: (constant) @module_name)
            "#
        }
        Lang::CSharp => {
            r#"
            (method_declaration name: (identifier) @fn_name)
            (class_declaration name: (identifier) @class_name)
            (interface_declaration name: (identifier) @interface_name)
            (struct_declaration name: (identifier) @struct_name)
            (enum_declaration name: (identifier) @enum_name)
            (namespace_declaration name: (identifier) @module_name)
            "#
        }
        Lang::Scala => {
            r#"
            (function_definition name: (identifier) @fn_name)
            (class_definition name: (identifier) @class_name)
            (trait_definition name: (identifier) @trait_name)
            (object_definition name: (identifier) @module_name)
            "#
        }
        Lang::Php => {
            r#"
            (function_definition name: (name) @fn_name)
            (method_declaration name: (name) @fn_name)
            (class_declaration name: (name) @class_name)
            (interface_declaration name: (name) @interface_name)
            (enum_declaration name: (name) @enum_name)
            "#
        }
        Lang::Lua => {
            r#"
            (function_declaration name: (identifier) @fn_name)
            "#
        }
        Lang::Zig => {
            r#"
            (function_declaration name: (identifier) @fn_name)
            "#
        }
        Lang::Elixir => {
            // Elixir tree-sitter grammar uses (call) nodes for def/defmodule
            ""
        }
        Lang::Julia => {
            // Julia AST uses nested structure without `name:` fields for functions/structs
            // Handled via walk-based extraction
            ""
        }
        Lang::Swift => {
            // Swift uses Java grammar as fallback — use Java queries
            r#"
            (method_declaration name: (identifier) @fn_name)
            (class_declaration name: (identifier) @class_name)
            (interface_declaration name: (identifier) @interface_name)
            "#
        }
    }
}

/// Fallback: walk the AST tree and extract symbols by node kind.
fn extract_symbols_walk(lang: Lang, root: Node, source: &[u8]) -> Vec<AstSymbol> {
    let mut symbols = Vec::new();
    let mut cursor = root.walk();
    walk_for_symbols(lang, &mut cursor, source, &mut symbols);
    symbols
}

fn walk_for_symbols(
    lang: Lang,
    cursor: &mut tree_sitter::TreeCursor,
    source: &[u8],
    symbols: &mut Vec<AstSymbol>,
) {
    loop {
        let node = cursor.node();
        let kind = node.kind();

        // Generic symbol detection by node kind name patterns
        let symbol = match lang {
            Lang::Julia => match kind {
                "function_definition" => {
                    // (function_definition (function) (signature (call_expression (identifier) (argument_list))) (end))
                    // The signature is child(1), call_expression is its child(0), identifier is its child(0)
                    let mut name_node = None;
                    for i in 0..node.child_count() {
                        let child = node.child(i).unwrap();
                        if child.kind() == "signature" {
                            // signature > call_expression > identifier
                            if let Some(call) = child.child(0) {
                                if call.kind() == "call_expression" {
                                    name_node = call.child(0);
                                }
                            }
                            break;
                        }
                    }
                    name_node
                        .and_then(|n| n.utf8_text(source).ok())
                        .filter(|n| {
                            !n.is_empty() && n.chars().next().map_or(false, |c| c.is_alphabetic())
                        })
                        .map(|n| (n.to_string(), SymbolKind::Function))
                }
                "struct_definition" => {
                    // (struct_definition (struct) (type_head (identifier)) ... (end))
                    // type_head is child(1), identifier is its child(0)
                    let mut name_node = None;
                    for i in 0..node.child_count() {
                        let child = node.child(i).unwrap();
                        if child.kind() == "type_head" {
                            name_node = child.child(0);
                            break;
                        }
                    }
                    name_node
                        .and_then(|n| n.utf8_text(source).ok())
                        .filter(|n| {
                            !n.is_empty() && n.chars().next().map_or(false, |c| c.is_alphabetic())
                        })
                        .map(|n| (n.to_string(), SymbolKind::Struct))
                }
                "module_definition" => {
                    // Find identifier child after "module" keyword
                    let mut name_node = None;
                    for i in 0..node.child_count() {
                        let child = node.child(i).unwrap();
                        if child.kind() == "identifier" {
                            name_node = Some(child);
                            break;
                        }
                    }
                    name_node
                        .and_then(|n| n.utf8_text(source).ok())
                        .filter(|n| {
                            !n.is_empty() && n.chars().next().map_or(false, |c| c.is_alphabetic())
                        })
                        .map(|n| (n.to_string(), SymbolKind::Module))
                }
                _ => None,
            },
            Lang::Elixir => match kind {
                "call" => {
                    // def foo, defp foo, defmodule Foo
                    let func_text = node.utf8_text(source).unwrap_or_default();
                    if func_text.starts_with("defmodule ") {
                        extract_elixir_name(func_text, "defmodule ")
                            .map(|n| (n, SymbolKind::Module))
                    } else if func_text.starts_with("def ") {
                        extract_elixir_name(func_text, "def ").map(|n| (n, SymbolKind::Function))
                    } else if func_text.starts_with("defp ") {
                        extract_elixir_name(func_text, "defp ").map(|n| (n, SymbolKind::Function))
                    } else {
                        None
                    }
                }
                _ => None,
            },
            _ => None,
        };

        if let Some((name, sk)) = symbol {
            symbols.push(AstSymbol {
                name,
                kind: sk,
                line: node.start_position().row + 1,
                doc_comment: None,
                parent_name: None,
                return_type: None,
                param_types: Vec::new(),
            });
        }

        if cursor.goto_first_child() {
            walk_for_symbols(lang, cursor, source, symbols);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn extract_elixir_name(text: &str, prefix: &str) -> Option<String> {
    let rest = text.strip_prefix(prefix)?.trim();
    let name: String = rest
        .chars()
        .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.' || *c == '?')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

// ---------------------------------------------------------------------------
// Import extraction
// ---------------------------------------------------------------------------

fn extract_imports(lang: Lang, ts_lang: &Language, root: Node, source: &[u8]) -> Vec<AstImport> {
    let query_src = import_query_for(lang);
    if query_src.is_empty() {
        return extract_imports_walk(lang, root, source);
    }
    let Ok(query) = Query::new(ts_lang, query_src) else {
        return extract_imports_walk(lang, root, source);
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source);
    let names = query.capture_names();

    let mut imports = Vec::new();
    let mut seen = HashSet::new();
    while let Some(m) = {
        matches.advance();
        matches.get()
    } {
        for cap in m.captures {
            let cap_name = &names[cap.index as usize];
            if *cap_name == "source" || *cap_name == "import_source" || *cap_name == "module" {
                let text = cap.node.utf8_text(source).unwrap_or_default();
                // Strip quotes
                let cleaned = text.trim_matches(|c| c == '\'' || c == '"');
                if cleaned.is_empty() || seen.contains(cleaned) {
                    continue;
                }
                seen.insert(cleaned.to_string());
                let is_relative = cleaned.starts_with('.') || cleaned.starts_with('/');
                imports.push(AstImport {
                    source: cleaned.to_string(),
                    line: cap.node.start_position().row + 1,
                    is_relative,
                });
            }
        }
    }
    imports
}

fn import_query_for(lang: Lang) -> &'static str {
    match lang {
        Lang::Python => {
            r#"
            (import_from_statement module_name: (dotted_name) @module)
            (import_statement name: (dotted_name) @module)
            "#
        }
        Lang::TypeScript | Lang::Tsx | Lang::JavaScript => {
            r#"
            (import_statement source: (string) @source)
            "#
        }
        Lang::Go => {
            r#"
            (import_spec path: (interpreted_string_literal) @source)
            "#
        }
        Lang::Rust => {
            r#"
            (use_declaration argument: (scoped_identifier) @module)
            (use_declaration argument: (identifier) @module)
            (use_declaration argument: (scoped_use_list path: (identifier) @module))
            "#
        }
        Lang::Java | Lang::Kotlin => {
            r#"
            (import_declaration (scoped_identifier) @module)
            "#
        }
        Lang::C | Lang::Cpp => {
            r#"
            (preproc_include path: (string_literal) @source)
            (preproc_include path: (system_lib_string) @source)
            "#
        }
        Lang::Ruby => {
            // require "foo" / require_relative "foo"
            ""
        }
        Lang::CSharp => {
            r#"
            (using_directive (identifier) @module)
            (using_directive (qualified_name) @module)
            "#
        }
        Lang::Scala => {
            r#"
            (import_declaration (identifier) @module)
            "#
        }
        Lang::Php => {
            r#"
            (namespace_use_declaration (namespace_use_clause (qualified_name) @module))
            "#
        }
        Lang::Swift => {
            r#"
            (import_declaration (scoped_identifier) @module)
            "#
        }
        Lang::Lua => {
            // require("foo") handled via call extraction
            ""
        }
        Lang::Zig => {
            // @import("foo") handled via walk
            ""
        }
        Lang::Elixir => {
            // use/import handled via walk
            ""
        }
        Lang::Julia => {
            r#"
            (import_statement (identifier) @module)
            (using_statement (identifier) @module)
            "#
        }
    }
}

fn extract_imports_walk(lang: Lang, root: Node, source: &[u8]) -> Vec<AstImport> {
    let mut imports = Vec::new();
    let mut cursor = root.walk();
    walk_for_imports(lang, &mut cursor, source, &mut imports);
    imports
}

fn walk_for_imports(
    lang: Lang,
    cursor: &mut tree_sitter::TreeCursor,
    source: &[u8],
    imports: &mut Vec<AstImport>,
) {
    loop {
        let node = cursor.node();
        let kind = node.kind();
        let text = node.utf8_text(source).unwrap_or_default();

        match lang {
            Lang::Ruby => {
                if kind == "call"
                    && (text.starts_with("require ") || text.starts_with("require_relative "))
                {
                    if let Some(arg) = extract_string_arg(text) {
                        let is_relative = text.starts_with("require_relative");
                        imports.push(AstImport {
                            source: arg,
                            line: node.start_position().row + 1,
                            is_relative,
                        });
                    }
                }
            }
            Lang::Lua => {
                if kind == "function_call" && text.starts_with("require") {
                    if let Some(arg) = extract_string_arg(text) {
                        imports.push(AstImport {
                            source: arg,
                            line: node.start_position().row + 1,
                            is_relative: false,
                        });
                    }
                }
            }
            Lang::Zig => {
                if kind == "IDENTIFIER" && text == "@import" {
                    // The parent contains the argument
                    // Try to get sibling string
                }
                if text.contains("@import") {
                    if let Some(arg) = extract_string_arg(text) {
                        let is_relative = arg.ends_with(".zig");
                        imports.push(AstImport {
                            source: arg,
                            line: node.start_position().row + 1,
                            is_relative,
                        });
                    }
                }
            }
            Lang::Elixir => {
                if kind == "call" {
                    if text.starts_with("use ")
                        || text.starts_with("import ")
                        || text.starts_with("alias ")
                    {
                        let prefix_len = if text.starts_with("use ") {
                            4
                        } else if text.starts_with("import ") {
                            7
                        } else {
                            6
                        };
                        let rest = text[prefix_len..].trim();
                        let module: String = rest
                            .chars()
                            .take_while(|c| c.is_alphanumeric() || *c == '.')
                            .collect();
                        if !module.is_empty() {
                            imports.push(AstImport {
                                source: module,
                                line: node.start_position().row + 1,
                                is_relative: false,
                            });
                        }
                    }
                }
            }
            _ => {}
        }

        if cursor.goto_first_child() {
            walk_for_imports(lang, cursor, source, imports);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

fn extract_string_arg(text: &str) -> Option<String> {
    // Extract first quoted string from text
    let start = text.find(|c| c == '"' || c == '\'')? + 1;
    let quote_char = text.as_bytes()[start - 1] as char;
    let rest = &text[start..];
    let end = rest.find(quote_char)?;
    let arg = &rest[..end];
    if arg.is_empty() {
        None
    } else {
        Some(arg.to_string())
    }
}

// ---------------------------------------------------------------------------
// Call graph extraction
// ---------------------------------------------------------------------------

fn extract_calls(lang: Lang, ts_lang: &Language, root: Node, source: &[u8]) -> Vec<AstCall> {
    let query_src = call_query_for(lang);
    if query_src.is_empty() {
        return extract_calls_walk(root, source);
    }
    let Ok(query) = Query::new(ts_lang, query_src) else {
        return extract_calls_walk(root, source);
    };
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, root, source);
    let names = query.capture_names();

    let mut calls = Vec::new();
    let mut seen = HashSet::new();
    while let Some(m) = {
        matches.advance();
        matches.get()
    } {
        for cap in m.captures {
            let cap_name = &names[cap.index as usize];
            if *cap_name == "callee" {
                let text = cap.node.utf8_text(source).unwrap_or_default().to_string();
                if text.is_empty() {
                    continue;
                }
                let key = (text.clone(), cap.node.start_position().row);
                if seen.contains(&key) {
                    continue;
                }
                seen.insert(key);
                calls.push(AstCall {
                    callee: text,
                    line: cap.node.start_position().row + 1,
                });
            }
        }
    }
    calls
}

fn call_query_for(lang: Lang) -> &'static str {
    match lang {
        Lang::Python => {
            r#"
            (call function: (identifier) @callee)
            (call function: (attribute attribute: (identifier) @callee))
            "#
        }
        Lang::TypeScript | Lang::Tsx | Lang::JavaScript => {
            r#"
            (call_expression function: (identifier) @callee)
            (call_expression function: (member_expression property: (property_identifier) @callee))
            "#
        }
        Lang::Go => {
            r#"
            (call_expression function: (identifier) @callee)
            (call_expression function: (selector_expression field: (field_identifier) @callee))
            "#
        }
        Lang::Rust => {
            r#"
            (call_expression function: (identifier) @callee)
            (call_expression function: (scoped_identifier name: (identifier) @callee))
            (call_expression function: (field_expression field: (field_identifier) @callee))
            "#
        }
        Lang::Java | Lang::Kotlin => {
            r#"
            (method_invocation name: (identifier) @callee)
            "#
        }
        Lang::C => {
            r#"
            (call_expression function: (identifier) @callee)
            "#
        }
        Lang::Cpp => {
            r#"
            (call_expression function: (identifier) @callee)
            (call_expression function: (qualified_identifier) @callee)
            (call_expression function: (field_expression field: (field_identifier) @callee))
            "#
        }
        Lang::Ruby => {
            // Ruby calls are complex — method_call, call, etc.
            ""
        }
        Lang::CSharp => {
            r#"
            (invocation_expression function: (identifier) @callee)
            (invocation_expression function: (member_access_expression name: (identifier) @callee))
            "#
        }
        Lang::Scala => {
            r#"
            (call_expression function: (identifier) @callee)
            "#
        }
        Lang::Php => {
            r#"
            (function_call_expression function: (name) @callee)
            (member_call_expression name: (name) @callee)
            "#
        }
        Lang::Lua => {
            r#"
            (function_call name: (identifier) @callee)
            "#
        }
        Lang::Zig => {
            // Zig call extraction via walk
            ""
        }
        Lang::Elixir | Lang::Julia | Lang::Swift => {
            // Fallback to walk
            ""
        }
    }
}

fn extract_calls_walk(root: Node, source: &[u8]) -> Vec<AstCall> {
    let mut calls = Vec::new();
    let mut cursor = root.walk();
    walk_for_calls(&mut cursor, source, &mut calls);
    calls
}

fn walk_for_calls(cursor: &mut tree_sitter::TreeCursor, source: &[u8], calls: &mut Vec<AstCall>) {
    loop {
        let node = cursor.node();
        let kind = node.kind();

        // Generic call detection
        if kind.contains("call") && !kind.contains("declaration") && !kind.contains("definition") {
            // Try to get the callee name from the first child
            if let Some(name_node) = node
                .child_by_field_name("function")
                .or_else(|| node.child_by_field_name("name"))
                .or_else(|| node.child(0))
            {
                let text = name_node.utf8_text(source).unwrap_or_default();
                // Extract just the function name (last identifier)
                let callee = text.rsplit('.').next().unwrap_or(text);
                let callee = callee.rsplit("::").next().unwrap_or(callee);
                if !callee.is_empty() && callee.chars().next().map_or(false, |c| c.is_alphabetic())
                {
                    calls.push(AstCall {
                        callee: callee.to_string(),
                        line: node.start_position().row + 1,
                    });
                }
            }
        }

        if cursor.goto_first_child() {
            walk_for_calls(cursor, source, calls);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Rationale comment extraction (language-agnostic)
// ---------------------------------------------------------------------------

fn extract_rationales(root: Node, source: &[u8]) -> Vec<AstRationale> {
    let mut rationales = Vec::new();
    let mut cursor = root.walk();
    walk_for_rationales(&mut cursor, source, &mut rationales);
    rationales
}

fn walk_for_rationales(
    cursor: &mut tree_sitter::TreeCursor,
    source: &[u8],
    rationales: &mut Vec<AstRationale>,
) {
    loop {
        let node = cursor.node();
        let kind = node.kind();

        if kind == "comment"
            || kind == "line_comment"
            || kind == "block_comment"
            || kind == "string"
            || kind == "heredoc_content"
        {
            let text = node.utf8_text(source).unwrap_or_default();
            // Check for rationale patterns
            for tag in &[
                "WHY",
                "NOTE",
                "IMPORTANT",
                "RATIONALE",
                "HACK",
                "FIXME",
                "TODO",
                "XXX",
            ] {
                if let Some(pos) = text.find(&format!("{tag}:")) {
                    let body = text[pos + tag.len() + 1..].trim();
                    // Strip trailing comment markers
                    let body = body.trim_end_matches("*/").trim_end_matches("-->").trim();
                    if !body.is_empty() {
                        rationales.push(AstRationale {
                            tag: tag.to_string(),
                            body: body.to_string(),
                            line: node.start_position().row + 1,
                        });
                    }
                }
            }
        }

        if cursor.goto_first_child() {
            walk_for_rationales(cursor, source, rationales);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// v2.2.2: Arrow function / const export extraction (JS/TS)
// ---------------------------------------------------------------------------

/// Extract `const foo = () => {}` and `const foo = function() {}` patterns.
fn extract_arrow_functions(root: Node, source: &[u8], symbols: &mut Vec<AstSymbol>) {
    let mut seen: HashSet<String> = symbols.iter().map(|s| s.name.clone()).collect();
    let mut cursor = root.walk();
    walk_for_arrow_functions(&mut cursor, source, symbols, &mut seen);
}

fn walk_for_arrow_functions(
    cursor: &mut tree_sitter::TreeCursor,
    source: &[u8],
    symbols: &mut Vec<AstSymbol>,
    seen: &mut HashSet<String>,
) {
    loop {
        let node = cursor.node();
        let kind = node.kind();

        // Match: (variable_declarator name: (identifier) value: (arrow_function | function))
        // Also: (lexical_declaration ... (variable_declarator ...))
        if kind == "variable_declarator" {
            if let (Some(name_node), Some(value_node)) = (
                node.child_by_field_name("name"),
                node.child_by_field_name("value"),
            ) {
                let val_kind = value_node.kind();
                if val_kind == "arrow_function"
                    || val_kind == "function"
                    || val_kind == "function_expression"
                {
                    let name = name_node.utf8_text(source).unwrap_or_default();
                    if !name.is_empty() && !seen.contains(name) {
                        seen.insert(name.to_string());
                        symbols.push(AstSymbol {
                            name: name.to_string(),
                            kind: SymbolKind::Function,
                            line: node.start_position().row + 1,
                            doc_comment: None,
                            parent_name: None,
                            return_type: None,
                            param_types: Vec::new(),
                        });
                    }
                }
            }
        }

        if cursor.goto_first_child() {
            walk_for_arrow_functions(cursor, source, symbols, seen);
            cursor.goto_parent();
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// v2.2.2: Doc comment population + containment (parent_name)
// ---------------------------------------------------------------------------

/// Walks the AST to populate `doc_comment` from preceding comment nodes
/// and `parent_name` from enclosing class/struct/trait/interface scope.
fn populate_doc_comments_and_containment(
    lang: Lang,
    root: Node,
    source: &[u8],
    symbols: &mut Vec<AstSymbol>,
) {
    if symbols.is_empty() {
        return;
    }

    // Build line→index map for fast symbol lookup
    let mut line_to_idx: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, sym) in symbols.iter().enumerate() {
        line_to_idx.entry(sym.line).or_default().push(i);
    }

    // Walk the tree to find containment and doc comments
    let mut scope_stack: Vec<(String, usize, usize)> = Vec::new(); // (name, start_row, end_row)
    walk_containment_and_docs(lang, root, source, &mut scope_stack, &line_to_idx, symbols);
}

fn walk_containment_and_docs(
    lang: Lang,
    node: Node,
    source: &[u8],
    scope_stack: &mut Vec<(String, usize, usize)>,
    line_to_idx: &std::collections::HashMap<usize, Vec<usize>>,
    symbols: &mut Vec<AstSymbol>,
) {
    let kind = node.kind();
    let row = node.start_position().row + 1;
    let end_row = node.end_position().row + 1;

    // Check if this node is a container (class/struct/trait/interface/enum/impl)
    let is_container = is_container_node(lang, kind);

    if is_container {
        // Find the name of this container
        if let Some(name) = extract_container_name(node, source) {
            scope_stack.push((name, row, end_row));
        }
    }

    // For symbol nodes at this line, assign parent_name from scope stack
    if let Some(indices) = line_to_idx.get(&row) {
        for &idx in indices {
            let sym = &mut symbols[idx];
            // Find innermost enclosing container that isn't this symbol itself
            for (parent_name, _start, end) in scope_stack.iter().rev() {
                if *parent_name != sym.name && row <= *end {
                    sym.parent_name = Some(parent_name.clone());
                    break;
                }
            }
        }
    }

    // Try to extract doc comment from preceding sibling
    if let Some(indices) = line_to_idx.get(&row) {
        if let Some(doc) = extract_preceding_doc_comment(node, source) {
            for &idx in indices {
                if symbols[idx].doc_comment.is_none() {
                    symbols[idx].doc_comment = Some(doc.clone());
                }
            }
        }
    }

    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_containment_and_docs(lang, child, source, scope_stack, line_to_idx, symbols);
        }
    }

    // Pop scope if we pushed one
    if is_container {
        if let Some(name) = extract_container_name(node, source) {
            if scope_stack.last().map(|(n, _, _)| n.as_str()) == Some(&name) {
                scope_stack.pop();
            }
        }
    }
}

// ---------------------------------------------------------------------------
// v2.2.2: Type signature extraction
// ---------------------------------------------------------------------------

/// Populate `return_type` and `param_types` on function/method symbols by
/// walking the tree-sitter AST and matching nodes by line number.
fn populate_type_signatures(
    lang: Lang,
    root: Node,
    source: &[u8],
    symbols: &mut [AstSymbol],
) {
    if symbols.is_empty() {
        return;
    }
    let mut line_to_idx: std::collections::HashMap<usize, Vec<usize>> =
        std::collections::HashMap::new();
    for (i, sym) in symbols.iter().enumerate() {
        if sym.kind == SymbolKind::Function {
            line_to_idx.entry(sym.line).or_default().push(i);
        }
    }
    if line_to_idx.is_empty() {
        return;
    }
    walk_type_signatures(lang, root, source, &line_to_idx, symbols);
}

fn walk_type_signatures(
    lang: Lang,
    node: Node,
    source: &[u8],
    line_to_idx: &std::collections::HashMap<usize, Vec<usize>>,
    symbols: &mut [AstSymbol],
) {
    let kind = node.kind();
    let row = node.start_position().row + 1;

    if is_function_node(lang, kind) {
        if let Some(indices) = line_to_idx.get(&row) {
            let (ret, params) = extract_type_info(lang, node, source);
            for &idx in indices {
                if symbols[idx].return_type.is_none() {
                    symbols[idx].return_type = ret.clone();
                }
                if symbols[idx].param_types.is_empty() {
                    symbols[idx].param_types = params.clone();
                }
            }
        }
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_type_signatures(lang, child, source, line_to_idx, symbols);
        }
    }
}

fn is_function_node(lang: Lang, kind: &str) -> bool {
    match lang {
        Lang::Rust => matches!(kind, "function_item"),
        Lang::TypeScript | Lang::Tsx => matches!(
            kind,
            "function_declaration" | "method_definition" | "arrow_function"
        ),
        Lang::JavaScript => matches!(
            kind,
            "function_declaration" | "method_definition" | "arrow_function"
        ),
        Lang::Python => matches!(kind, "function_definition"),
        Lang::Go => matches!(kind, "function_declaration" | "method_declaration"),
        Lang::Java | Lang::Kotlin => matches!(kind, "method_declaration"),
        _ => false,
    }
}

fn extract_type_info(
    lang: Lang,
    node: Node,
    source: &[u8],
) -> (Option<String>, Vec<(String, String)>) {
    let ret = extract_return_type(lang, node, source);
    let params = extract_param_types(lang, node, source);
    (ret, params)
}

fn extract_return_type(lang: Lang, node: Node, source: &[u8]) -> Option<String> {
    match lang {
        Lang::Rust => {
            // Rust: function_item → return_type child (a type node)
            node.child_by_field_name("return_type")
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
        }
        Lang::TypeScript | Lang::Tsx => {
            // TS: return_type is a "type_annotation" child
            node.child_by_field_name("return_type")
                .map(|n| n.utf8_text(source).unwrap_or_default().trim_start_matches(':').trim().to_string())
        }
        Lang::Python => {
            // Python: function_definition → return_type child
            node.child_by_field_name("return_type")
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
        }
        Lang::Go => {
            // Go: function_declaration → result child
            node.child_by_field_name("result")
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
        }
        Lang::Java | Lang::Kotlin => {
            // Java: method_declaration → type child (first non-modifier child)
            node.child_by_field_name("type")
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
        }
        _ => None,
    }
}

fn extract_param_types(lang: Lang, node: Node, source: &[u8]) -> Vec<(String, String)> {
    let params_node = match lang {
        Lang::Rust => node.child_by_field_name("parameters"),
        Lang::TypeScript | Lang::Tsx | Lang::JavaScript => node.child_by_field_name("parameters"),
        Lang::Python => node.child_by_field_name("parameters"),
        Lang::Go => node.child_by_field_name("parameters"),
        Lang::Java | Lang::Kotlin => node.child_by_field_name("parameters"),
        _ => None,
    };
    let Some(params) = params_node else {
        return Vec::new();
    };
    let mut result = Vec::new();
    for i in 0..params.child_count() {
        let Some(child) = params.child(i) else { continue };
        let ck = child.kind();
        // Skip punctuation (commas, parens)
        if ck == "," || ck == "(" || ck == ")" {
            continue;
        }
        let (name, ty) = extract_single_param(lang, child, source);
        if !name.is_empty() && !ty.is_empty() {
            result.push((name, ty));
        }
    }
    result
}

fn extract_single_param(lang: Lang, node: Node, source: &[u8]) -> (String, String) {
    match lang {
        Lang::Rust => {
            // parameter → pattern: type
            let name = node.child_by_field_name("pattern")
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
                .unwrap_or_default();
            let ty = node.child_by_field_name("type")
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
                .unwrap_or_default();
            (name, ty)
        }
        Lang::TypeScript | Lang::Tsx => {
            // required_parameter | optional_parameter → pattern + type_annotation
            let name = node.child_by_field_name("pattern")
                .or_else(|| node.child_by_field_name("name"))
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
                .unwrap_or_default();
            let ty = node.child_by_field_name("type")
                .map(|n| n.utf8_text(source).unwrap_or_default().trim_start_matches(':').trim().to_string())
                .unwrap_or_default();
            (name, ty)
        }
        Lang::Python => {
            // typed_parameter or identifier; type in annotation child
            let name = node.child_by_field_name("name")
                .or_else(|| {
                    // Simple identifier parameter
                    if node.kind() == "identifier" {
                        Some(node)
                    } else {
                        node.child(0)
                    }
                })
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
                .unwrap_or_default();
            let ty = node.child_by_field_name("type")
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
                .unwrap_or_default();
            (name, ty)
        }
        Lang::Go => {
            // parameter_declaration → name type
            let name = node.child_by_field_name("name")
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
                .unwrap_or_default();
            let ty = node.child_by_field_name("type")
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
                .unwrap_or_default();
            (name, ty)
        }
        Lang::Java | Lang::Kotlin => {
            let name = node.child_by_field_name("name")
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
                .unwrap_or_default();
            let ty = node.child_by_field_name("type")
                .map(|n| n.utf8_text(source).unwrap_or_default().to_string())
                .unwrap_or_default();
            (name, ty)
        }
        _ => (String::new(), String::new()),
    }
}

/// Check if a tree-sitter node kind represents a container (class, struct, etc.)
fn is_container_node(lang: Lang, kind: &str) -> bool {
    match lang {
        Lang::Python => matches!(kind, "class_definition"),
        Lang::TypeScript | Lang::Tsx | Lang::JavaScript => {
            matches!(kind, "class_declaration" | "class")
        }
        Lang::Rust => matches!(kind, "struct_item" | "enum_item" | "trait_item" | "impl_item"),
        Lang::Go => matches!(kind, "type_declaration"),
        Lang::Java | Lang::Kotlin | Lang::Swift => {
            matches!(kind, "class_declaration" | "interface_declaration" | "enum_declaration")
        }
        Lang::CSharp => matches!(
            kind,
            "class_declaration" | "struct_declaration" | "interface_declaration" | "enum_declaration"
        ),
        Lang::Cpp => matches!(kind, "class_specifier" | "struct_specifier"),
        Lang::Ruby => matches!(kind, "class" | "module"),
        Lang::Scala => matches!(kind, "class_definition" | "trait_definition" | "object_definition"),
        Lang::Php => matches!(kind, "class_declaration" | "interface_declaration"),
        _ => false,
    }
}

/// Extract the name of a container node (class, struct, trait, etc.)
fn extract_container_name(node: Node, source: &[u8]) -> Option<String> {
    // Try common child field names for the container's name
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.to_string())
        .or_else(|| {
            // Rust impl_item: (impl_item type: (type_identifier))
            node.child_by_field_name("type")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string())
        })
        .or_else(|| {
            // Rust impl_item with trait: (impl_item trait: (type_identifier) type: ...)
            node.child_by_field_name("trait")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.to_string())
        })
}

/// Extract doc comment from the previous sibling node(s).
fn extract_preceding_doc_comment(node: Node, source: &[u8]) -> Option<String> {
    let mut prev = node.prev_sibling()?;
    let mut lines = Vec::new();

    // Collect consecutive comment nodes immediately preceding this node
    loop {
        let kind = prev.kind();
        if kind == "comment" || kind == "line_comment" || kind == "block_comment" {
            let text = prev.utf8_text(source).unwrap_or_default();
            // Strip comment markers
            let cleaned = text
                .trim_start_matches("///")
                .trim_start_matches("//!")
                .trim_start_matches("//")
                .trim_start_matches("/*")
                .trim_end_matches("*/")
                .trim_start_matches('#')
                .trim_start_matches("\"\"\"")
                .trim_end_matches("\"\"\"")
                .trim();
            if !cleaned.is_empty() {
                lines.push(cleaned.to_string());
            }
        } else {
            break;
        }
        match prev.prev_sibling() {
            Some(p) => prev = p,
            None => break,
        }
    }

    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    Some(lines.join(" "))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_python_extraction() {
        let source = r#"
import os
from pathlib import Path

class DataProcessor:
    """Process data files."""
    # WHY: batch processing is more efficient
    def process(self, items):
        result = self.validate(items)
        return transform(result)

def helper():
    pass
"#;
        let ext = extract_ast("py", source).unwrap();
        assert_eq!(ext.language, "python");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "DataProcessor" && s.kind == SymbolKind::Class)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "process" && s.kind == SymbolKind::Function)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "helper" && s.kind == SymbolKind::Function)
        );
        assert!(ext.imports.iter().any(|i| i.source == "os"));
        assert!(ext.imports.iter().any(|i| i.source == "pathlib"));
        assert!(
            ext.calls
                .iter()
                .any(|c| c.callee == "validate" || c.callee == "transform")
        );
        assert!(ext.rationales.iter().any(|r| r.tag == "WHY"));
    }

    #[test]
    fn test_typescript_extraction() {
        let source = r#"
import { Router } from 'express';
import { Logger } from './logger';

interface Config {
    port: number;
}

class Server {
    // WHY: singleton prevents duplicate listeners
    start(): void {
        this.listen();
        console.log("started");
    }
}

function createApp(): Server {
    return new Server();
}
"#;
        let ext = extract_ast("ts", source).unwrap();
        assert_eq!(ext.language, "typescript");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Config" && s.kind == SymbolKind::Interface)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Server" && s.kind == SymbolKind::Class)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "createApp" && s.kind == SymbolKind::Function)
        );
        assert!(ext.imports.iter().any(|i| i.source == "express"));
        assert!(
            ext.imports
                .iter()
                .any(|i| i.source == "./logger" && i.is_relative)
        );
        assert!(ext.rationales.iter().any(|r| r.tag == "WHY"));
    }

    #[test]
    fn test_rust_extraction() {
        let source = r#"
use std::collections::HashMap;
use crate::config::Settings;

// WHY: avoid allocations on hot path
const MAX_BUFFER: usize = 1024;

struct Parser {
    buffer: Vec<u8>,
}

trait Tokenizer {
    fn tokenize(&self, input: &str) -> Vec<String>;
}

enum Token {
    Ident(String),
    Number(f64),
}

fn parse(input: &str) -> Vec<Token> {
    let parser = Parser { buffer: Vec::new() };
    parser.tokenize(input)
}
"#;
        let ext = extract_ast("rs", source).unwrap();
        assert_eq!(ext.language, "rust");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Parser" && s.kind == SymbolKind::Struct)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Tokenizer" && s.kind == SymbolKind::Trait)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Token" && s.kind == SymbolKind::Enum)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "parse" && s.kind == SymbolKind::Function)
        );
        assert!(
            ext.imports
                .iter()
                .any(|i| i.source.contains("std") || i.source.contains("HashMap"))
        );
        assert!(ext.calls.iter().any(|c| c.callee == "tokenize"));
        assert!(ext.rationales.iter().any(|r| r.tag == "WHY"));
    }

    #[test]
    fn test_go_extraction() {
        let source = r#"
package main

import (
    "fmt"
    "net/http"
)

type Server struct {
    port int
}

// NOTE: handler must be concurrent-safe
func (s *Server) Start() {
    fmt.Println("starting")
    http.ListenAndServe(":8080", nil)
}

func NewServer(port int) *Server {
    return &Server{port: port}
}
"#;
        let ext = extract_ast("go", source).unwrap();
        assert_eq!(ext.language, "go");
        assert!(ext.symbols.iter().any(|s| s.name == "Server"));
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Start" && s.kind == SymbolKind::Function)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "NewServer" && s.kind == SymbolKind::Function)
        );
        assert!(ext.imports.iter().any(|i| i.source.contains("fmt")));
        assert!(ext.imports.iter().any(|i| i.source.contains("net/http")));
        assert!(ext.rationales.iter().any(|r| r.tag == "NOTE"));
    }

    #[test]
    fn test_java_extraction() {
        let source = r#"
import java.util.List;
import java.util.HashMap;

public class App {
    // WHY: lazy init saves memory
    private List<String> items;

    public void run() {
        initialize();
        process();
    }

    public interface Callback {
        void onComplete();
    }
}
"#;
        let ext = extract_ast("java", source).unwrap();
        assert_eq!(ext.language, "java");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "App" && s.kind == SymbolKind::Class)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "run" && s.kind == SymbolKind::Function)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Callback" && s.kind == SymbolKind::Interface)
        );
        assert!(ext.imports.len() >= 2);
        assert!(
            ext.calls
                .iter()
                .any(|c| c.callee == "initialize" || c.callee == "process")
        );
    }

    #[test]
    fn test_c_extraction() {
        let source = r#"
#include <stdio.h>
#include "utils.h"

// WHY: avoid stack overflow on large inputs
#define MAX_SIZE 1024

struct Buffer {
    char data[MAX_SIZE];
    int length;
};

typedef int ErrorCode;

void process(struct Buffer *buf) {
    validate(buf);
    printf("done\n");
}

int main() {
    struct Buffer buf;
    process(&buf);
    return 0;
}
"#;
        let ext = extract_ast("c", source).unwrap();
        assert_eq!(ext.language, "c");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Buffer" && s.kind == SymbolKind::Struct)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "process" && s.kind == SymbolKind::Function)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "main" && s.kind == SymbolKind::Function)
        );
        assert!(ext.imports.iter().any(|i| i.source.contains("stdio")));
        assert!(ext.imports.iter().any(|i| i.source.contains("utils")));
        assert!(
            ext.calls
                .iter()
                .any(|c| c.callee == "validate" || c.callee == "printf" || c.callee == "process")
        );
    }

    #[test]
    fn test_cpp_extraction() {
        let source = r#"
#include <iostream>
#include <vector>

namespace engine {

class Renderer {
public:
    // NOTE: must call init before render
    void render() {
        prepare();
        draw();
    }
};

struct Config {
    int width;
    int height;
};

} // namespace engine
"#;
        let ext = extract_ast("cpp", source).unwrap();
        assert_eq!(ext.language, "cpp");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "engine" && s.kind == SymbolKind::Module)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Renderer" && s.kind == SymbolKind::Class)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Config" && s.kind == SymbolKind::Struct)
        );
        assert!(ext.imports.len() >= 2);
    }

    #[test]
    fn test_csharp_extraction() {
        let source = r#"
using System;
using System.Collections.Generic;

namespace MyApp {
    public class Service {
        // WHY: thread safety
        public void Execute() {
            Validate();
            Process();
        }
    }

    public interface IHandler {
        void Handle();
    }
}
"#;
        let ext = extract_ast("cs", source).unwrap();
        assert_eq!(ext.language, "csharp");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Service" && s.kind == SymbolKind::Class)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Execute" && s.kind == SymbolKind::Function)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "IHandler" && s.kind == SymbolKind::Interface)
        );
    }

    #[test]
    fn test_php_extraction() {
        let source = r#"<?php
namespace App\Controllers;

use App\Models\User;
use App\Services\AuthService;

class UserController {
    // WHY: prevent unauthorized access
    public function index() {
        $users = $this->getAll();
        return $this->render($users);
    }

    public function show($id) {
        return User::find($id);
    }
}
"#;
        let ext = extract_ast("php", source).unwrap();
        assert_eq!(ext.language, "php");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "UserController" && s.kind == SymbolKind::Class)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "index" && s.kind == SymbolKind::Function)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "show" && s.kind == SymbolKind::Function)
        );
    }

    #[test]
    fn test_scala_extraction() {
        let source = r#"
import scala.collection.mutable
import scala.util.Try

class Processor {
    // WHY: immutable transforms are safer
    def process(items: List[String]): List[String] = {
        items.map(transform)
    }

    def transform(item: String): String = item.trim
}

trait Handler {
    def handle(): Unit
}

object Main {
    def main(args: Array[String]): Unit = {
        val p = new Processor()
        p.process(List("a", "b"))
    }
}
"#;
        let ext = extract_ast("scala", source).unwrap();
        assert_eq!(ext.language, "scala");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Processor" && s.kind == SymbolKind::Class)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Handler" && s.kind == SymbolKind::Trait)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Main" && s.kind == SymbolKind::Module)
        );
    }

    #[test]
    fn test_lua_extraction() {
        let source = r#"
local json = require("json")
local utils = require("utils")

-- WHY: cache prevents redundant parsing
local cache = {}

function parse(input)
    if cache[input] then
        return cache[input]
    end
    local result = json.decode(input)
    cache[input] = result
    return result
end
"#;
        let ext = extract_ast("lua", source).unwrap();
        assert_eq!(ext.language, "lua");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "parse" && s.kind == SymbolKind::Function)
        );
        assert!(ext.imports.iter().any(|i| i.source == "json"));
        assert!(ext.imports.iter().any(|i| i.source == "utils"));
        assert!(ext.rationales.iter().any(|r| r.tag == "WHY"));
    }

    #[test]
    fn test_zig_extraction() {
        let source = r#"
const std = @import("std");
const mem = @import("std").mem;

// WHY: manual memory management for performance
const Allocator = std.mem.Allocator;

fn process(allocator: Allocator, data: []const u8) !void {
    const result = try parse(data);
    try validate(result);
}

fn parse(data: []const u8) ![]u8 {
    return data;
}
"#;
        let ext = extract_ast("zig", source).unwrap();
        assert_eq!(ext.language, "zig");
        assert!(ext.symbols.len() >= 2); // process, parse
        assert!(ext.rationales.iter().any(|r| r.tag == "WHY"));
    }

    #[test]
    fn test_ruby_extraction() {
        let source = r#"
require 'json'
require_relative 'helpers'

class Handler
  # WHY: memoize for performance
  def process(data)
    validated = validate(data)
    transform(validated)
  end

  def validate(data)
    data
  end
end

module Utils
  def self.format(text)
    text.strip
  end
end
"#;
        let ext = extract_ast("rb", source).unwrap();
        assert_eq!(ext.language, "ruby");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Handler" && s.kind == SymbolKind::Class)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "process" && s.kind == SymbolKind::Function)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Utils" && s.kind == SymbolKind::Module)
        );
        assert!(ext.imports.iter().any(|i| i.source == "json"));
        assert!(
            ext.imports
                .iter()
                .any(|i| i.source == "helpers" && i.is_relative)
        );
    }

    #[test]
    fn test_julia_extraction() {
        let source = r#"
import LinearAlgebra
using Statistics

# WHY: in-place operations save memory
struct DataPoint
    x::Float64
    y::Float64
end

function process(points::Vector{DataPoint})
    mean_x = mean([p.x for p in points])
    return normalize(points, mean_x)
end

function normalize(points, center)
    return points
end
"#;
        let ext = extract_ast("jl", source).unwrap();
        assert_eq!(ext.language, "julia");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "DataPoint" && s.kind == SymbolKind::Struct)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "process" && s.kind == SymbolKind::Function)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "normalize" && s.kind == SymbolKind::Function)
        );
    }

    #[test]
    fn test_unsupported_extension() {
        assert!(extract_ast("xyz", "some content").is_none());
    }

    #[test]
    fn test_empty_source() {
        let ext = extract_ast("py", "").unwrap();
        assert!(ext.symbols.is_empty());
        assert!(ext.imports.is_empty());
        assert!(ext.calls.is_empty());
        assert!(ext.rationales.is_empty());
    }

    #[test]
    fn test_tsx_extraction() {
        let source = r#"
import React from 'react';
import { Button } from './components';

interface Props {
    title: string;
}

// WHY: memo prevents unnecessary re-renders
function App({ title }: Props) {
    const handleClick = () => {
        console.log("clicked");
        doSomething();
    };
    return <Button onClick={handleClick}>{title}</Button>;
}

export default App;
"#;
        let ext = extract_ast("tsx", source).unwrap();
        assert_eq!(ext.language, "tsx");
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Props" && s.kind == SymbolKind::Interface)
        );
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "App" && s.kind == SymbolKind::Function)
        );
        assert!(ext.imports.iter().any(|i| i.source == "react"));
        assert!(
            ext.imports
                .iter()
                .any(|i| i.source == "./components" && i.is_relative)
        );
    }

    #[test]
    fn test_elixir_extraction() {
        let source = r#"
defmodule Worker do
  use GenServer
  import Logger

  # WHY: supervised restart handles crashes
  def start_link(opts) do
    GenServer.start_link(__MODULE__, opts)
  end

  def handle_call(:status, _from, state) do
    {:reply, :ok, state}
  end
end
"#;
        let ext = extract_ast("ex", source).unwrap();
        assert_eq!(ext.language, "elixir");
        // Elixir extraction is walk-based, verify we get something
        assert!(
            ext.symbols
                .iter()
                .any(|s| s.name == "Worker" || s.name.contains("start_link"))
        );
        assert!(ext.rationales.iter().any(|r| r.tag == "WHY"));
    }

    #[test]
    fn test_all_supported_extensions() {
        let extensions = vec![
            "py", "ts", "tsx", "js", "jsx", "mjs", "cjs", "go", "rs", "java", "c", "h", "cc",
            "cpp", "rb", "cs", "kt", "scala", "php", "swift", "lua", "zig", "ex", "exs", "jl", "m",
        ];
        for ext in extensions {
            assert!(
                Lang::from_extension(ext).is_some(),
                "Extension .{ext} should be supported"
            );
        }
    }

    #[test]
    fn test_rationale_tags() {
        let source = r#"
// WHY: performance optimization
// NOTE: this is temporary
// IMPORTANT: do not modify
// TODO: refactor later
// FIXME: broken on edge case
fn test() {}
"#;
        let ext = extract_ast("rs", source).unwrap();
        let tags: Vec<&str> = ext.rationales.iter().map(|r| r.tag.as_str()).collect();
        assert!(tags.contains(&"WHY"));
        assert!(tags.contains(&"NOTE"));
        assert!(tags.contains(&"IMPORTANT"));
        assert!(tags.contains(&"TODO"));
        assert!(tags.contains(&"FIXME"));
    }

    // v2.2.2 tests

    #[test]
    fn test_containment_edges_python() {
        let source = r#"
class DataProcessor:
    def process(self):
        pass
    def validate(self):
        pass

def standalone():
    pass
"#;
        let ext = extract_ast("py", source).unwrap();
        let process = ext.symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(process.parent_name.as_deref(), Some("DataProcessor"));
        let validate = ext.symbols.iter().find(|s| s.name == "validate").unwrap();
        assert_eq!(validate.parent_name.as_deref(), Some("DataProcessor"));
        let standalone = ext.symbols.iter().find(|s| s.name == "standalone").unwrap();
        assert!(standalone.parent_name.is_none());
    }

    #[test]
    fn test_containment_edges_rust() {
        let source = r#"
struct MyStruct;
impl MyStruct {
    fn method_a(&self) {}
    fn method_b(&self) {}
}
fn free_function() {}
"#;
        let ext = extract_ast("rs", source).unwrap();
        let method_a = ext.symbols.iter().find(|s| s.name == "method_a").unwrap();
        assert_eq!(method_a.parent_name.as_deref(), Some("MyStruct"));
        let free_fn = ext.symbols.iter().find(|s| s.name == "free_function").unwrap();
        assert!(free_fn.parent_name.is_none());
    }

    #[test]
    fn test_arrow_function_extraction_js() {
        let source = r#"
const handler = () => {};
const processData = function() {};
export const CONFIG = 42;
function normalFunc() {}
"#;
        let ext = extract_ast("js", source).unwrap();
        let names: Vec<&str> = ext.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"handler"), "should extract arrow function: {names:?}");
        assert!(names.contains(&"processData"), "should extract function expression: {names:?}");
        assert!(names.contains(&"normalFunc"), "should still extract normal functions: {names:?}");
    }

    #[test]
    fn test_arrow_function_extraction_ts() {
        let source = r#"
export const fetchUser = async (id: string): Promise<User> => {
    return await db.get(id);
};
const validate = (input: string): boolean => input.length > 0;
function regularFn(): void {}
"#;
        let ext = extract_ast("ts", source).unwrap();
        let names: Vec<&str> = ext.symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"fetchUser"), "should extract async arrow: {names:?}");
        assert!(names.contains(&"validate"), "should extract arrow: {names:?}");
        assert!(names.contains(&"regularFn"), "should extract regular fn: {names:?}");
    }

    #[test]
    fn test_doc_comment_extraction_rust() {
        let source = r#"
/// Processes data from input.
fn process_data() {}

// Regular comment
fn other_fn() {}
"#;
        let ext = extract_ast("rs", source).unwrap();
        let process = ext.symbols.iter().find(|s| s.name == "process_data").unwrap();
        assert!(process.doc_comment.is_some(), "should extract doc comment");
        assert!(process.doc_comment.as_ref().unwrap().contains("Processes data"));
    }

    // ── v2.2.2: Type signature extraction tests ──

    #[test]
    fn test_type_signatures_rust() {
        let source = r#"
fn process(data: &[u8], count: usize) -> Result<Vec<String>> {
    todo!()
}
fn no_return() {}
"#;
        let ext = extract_ast("rs", source).unwrap();
        let process = ext.symbols.iter().find(|s| s.name == "process").unwrap();
        assert_eq!(
            process.return_type.as_deref(),
            Some("Result<Vec<String>>"),
            "should extract Rust return type"
        );
        assert!(
            process.param_types.len() >= 2,
            "should extract params: {:?}",
            process.param_types
        );

        let no_ret = ext.symbols.iter().find(|s| s.name == "no_return").unwrap();
        assert!(no_ret.return_type.is_none(), "no return type for void fn");
    }

    #[test]
    fn test_type_signatures_typescript() {
        let source = r#"
function greet(name: string, age: number): string {
  return name;
}
"#;
        let ext = extract_ast("ts", source).unwrap();
        let greet = ext.symbols.iter().find(|s| s.name == "greet").unwrap();
        assert!(
            greet.return_type.is_some(),
            "should extract TS return type: {:?}",
            greet
        );
        assert!(
            greet.param_types.len() >= 2,
            "should extract TS params: {:?}",
            greet.param_types
        );
    }

    #[test]
    fn test_type_signatures_python() {
        let source = r#"
def compute(x: int, y: float) -> str:
    return str(x + y)
"#;
        let ext = extract_ast("py", source).unwrap();
        let compute = ext.symbols.iter().find(|s| s.name == "compute").unwrap();
        assert!(
            compute.return_type.is_some(),
            "should extract Python return type: {:?}",
            compute
        );
    }

    #[test]
    fn test_type_signatures_go() {
        let source = r#"
package main
func Process(data []byte, count int) (string, error) {
    return "", nil
}
"#;
        let ext = extract_ast("go", source).unwrap();
        let process = ext.symbols.iter().find(|s| s.name == "Process").unwrap();
        assert!(
            process.return_type.is_some(),
            "should extract Go return type: {:?}",
            process
        );
    }
}
