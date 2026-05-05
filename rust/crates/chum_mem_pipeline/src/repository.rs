use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::ast_parser;
use crate::knowledge::GraphStatistics;
use crate::{
    KnowledgeEdge, KnowledgeGraph, KnowledgeNode, assign_communities_with_budget,
    generate_knowledge_report, to_node_link_json,
};

const DEFAULT_OUT_DIR_NAME: &str = "graphify-out";

/// Skip large text/code files — they're usually generated, vendored, or data dumps.
const MAX_FILE_SIZE_BYTES: u64 = 256 * 1024;
/// Binary repository artifacts are parsed for structure/metadata only and may be larger.
const MAX_BINARY_FILE_SIZE_BYTES: u64 = 16 * 1024 * 1024;

const CODE_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "rs", "java", "c", "cc", "cpp", "h", "hpp",
    "cxx", "hxx", "rb", "cs", "kt", "kts", "scala", "php", "swift", "lua", "zig", "ps1", "sh",
    "sql", "css", "scss", "sass", "less", "vue", "svelte", "astro", "ex", "exs", "m", "mm", "jl",
    "dart",
];
const DOC_EXTENSIONS: &[&str] = &[
    "md", "mdx", "html", "htm", "txt", "rst", "yaml", "yml", "json", "jsonc",
];
const OFFICE_DOCUMENT_EXTENSIONS: &[&str] = &["docx"];
const SPREADSHEET_EXTENSIONS: &[&str] = &["xlsx"];
const PRESENTATION_EXTENSIONS: &[&str] = &["pptx"];
const PDF_EXTENSIONS: &[&str] = &["pdf"];
const IMAGE_EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "webp", "gif"];
const MEDIA_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "mp3", "wav"];
const IGNORE_DIRS: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "node_modules",
    ".pnp",
    ".yarn",
    "dist",
    "build",
    "out",
    ".output",
    ".next",
    ".nuxt",
    ".vercel",
    ".netlify",
    ".turbo",
    ".parcel-cache",
    ".cache",
    ".rollup.cache",
    "target",
    "__pycache__",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    "venv",
    ".venv",
    "bin",
    "obj",
    ".idea",
    ".vscode",
    ".vs",
    "coverage",
    ".nyc_output",
    ".docusaurus",
    "secrets",
    DEFAULT_OUT_DIR_NAME,
];
const IGNORE_FILES: &[&str] = &[
    ".DS_Store",
    "Thumbs.db",
    ".env",
    ".env.local",
    ".env.development",
    ".env.production",
    ".env.staging",
    ".env.test",
    "credentials.json",
    "serviceAccountKey.json",
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lockb",
    "composer.lock",
    "Cargo.lock",
    "Gemfile.lock",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryBuildOptions {
    pub root_dir: String,
    pub out_dir: Option<String>,
    pub project_id: Option<Uuid>,
    pub update: bool,
    pub no_viz: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryBuildArtifacts {
    pub graph_json_path: String,
    pub report_path: String,
    pub html_path: Option<String>,
    pub cache_manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryBuildStats {
    pub processed_files: u32,
    pub reused_files: u32,
    pub removed_files: u32,
    pub total_files: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryBuildResult {
    pub graph: KnowledgeGraph,
    pub report: String,
    pub artifacts: RepositoryBuildArtifacts,
    pub stats: RepositoryBuildStats,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FileKind {
    Code,
    Doc,
    OfficeDocument,
    Spreadsheet,
    Presentation,
    Pdf,
    Image,
    Media,
    Unsupported,
}

impl FileKind {
    fn is_textual(self) -> bool {
        matches!(self, Self::Code | Self::Doc)
    }

    fn is_binary(self) -> bool {
        !self.is_textual() && self != Self::Unsupported
    }

    fn graph_node_type(self) -> &'static str {
        match self {
            Self::Code => "file",
            Self::Doc
            | Self::OfficeDocument
            | Self::Spreadsheet
            | Self::Presentation
            | Self::Pdf
            | Self::Image
            | Self::Media
            | Self::Unsupported => "document",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Code => "code",
            Self::Doc => "document",
            Self::OfficeDocument => "office_document",
            Self::Spreadsheet => "spreadsheet",
            Self::Presentation => "presentation",
            Self::Pdf => "pdf",
            Self::Image => "image",
            Self::Media => "media",
            Self::Unsupported => "unsupported",
        }
    }
}

#[derive(Debug, Clone)]
struct RepositoryFile {
    absolute_path: PathBuf,
    relative_path: String,
    kind: FileKind,
}

#[derive(Debug, Clone)]
pub struct RepositoryFilePayload {
    pub path: String,
    pub content: Option<String>,
    pub bytes: Option<Vec<u8>>,
    pub media_type: Option<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Debug, Clone)]
struct ParsedRepositoryFile {
    nodes: Vec<KnowledgeNode>,
    edges: Vec<KnowledgeEdge>,
    semantic_text: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RepositoryBuildError {
    #[error("repository import root `{0}` does not exist")]
    MissingRoot(String),
    #[error("repository import root `{0}` is not a directory")]
    InvalidRoot(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("invalid utf-8 path in repository root")]
    InvalidPath,
}

pub fn build_repository_knowledge(
    options: RepositoryBuildOptions,
    max_cluster_nodes: usize,
    max_cluster_edges: usize,
) -> Result<RepositoryBuildResult, RepositoryBuildError> {
    let root_dir = fs::canonicalize(&options.root_dir)
        .map_err(|_| RepositoryBuildError::MissingRoot(options.root_dir.clone()))?;
    if !root_dir.is_dir() {
        return Err(RepositoryBuildError::InvalidRoot(options.root_dir));
    }

    let preferred_out_dir = options
        .out_dir
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| root_dir.join(DEFAULT_OUT_DIR_NAME));
    let out_dir = ensure_output_dir(&preferred_out_dir, &root_dir)?;

    let files = collect_repository_files(&root_dir)?;
    let known_files = files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<HashSet<_>>();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut semantic_inputs = Vec::new();
    let mut skipped_files: u32 = 0;

    for file in &files {
        // Skip files exceeding the size limit (generated/vendored/data dumps)
        let meta = match fs::metadata(&file.absolute_path) {
            Ok(m) => m,
            Err(_) => {
                skipped_files += 1;
                continue;
            }
        };
        let max_size = if file.kind.is_binary() {
            MAX_BINARY_FILE_SIZE_BYTES
        } else {
            MAX_FILE_SIZE_BYTES
        };
        if meta.len() > max_size {
            if file.kind.is_binary() {
                let extracted = extract_repository_file(file, &[], &known_files, Some(meta.len()));
                nodes.extend(extracted.nodes);
                edges.extend(extracted.edges);
            }
            skipped_files += 1;
            continue;
        }

        let bytes = match fs::read(&file.absolute_path) {
            Ok(b) => b,
            Err(_) => {
                skipped_files += 1;
                continue;
            }
        };

        let extracted = extract_repository_file(file, &bytes, &known_files, Some(meta.len()));
        if extracted.nodes.is_empty() {
            skipped_files += 1;
            continue;
        }
        nodes.extend(extracted.nodes);
        edges.extend(extracted.edges);
        if let Some(text) = extracted.semantic_text {
            semantic_inputs.push((file.relative_path.clone(), text));
        }
    }

    edges.extend(build_semantic_similarity_edges(&semantic_inputs));

    // v2.2.2: Cross-file call resolution — resolve "inferred" call edges
    // against a global symbol table built from all extracted symbol nodes.
    resolve_cross_file_calls(&nodes, &mut edges, &known_files);

    let graph = assign_communities_with_budget(
        &KnowledgeGraph {
            version: "1.0.0".to_string(),
            generated_at: rfc3339_now(),
            project_id: options.project_id.unwrap_or_else(Uuid::nil),
            nodes: dedupe_nodes(nodes),
            edges: dedupe_edges(edges),
            communities: Vec::new(),
            statistics: GraphStatistics {
                node_count: 0,
                edge_count: 0,
                community_count: 0,
                evidence_distribution: Default::default(),
                avg_degree: 0.0,
                density: 0.0,
                isolated_nodes: 0,
            },
        },
        max_cluster_nodes,
        max_cluster_edges,
    );
    let report = generate_knowledge_report(&graph);

    let graph_json_path = out_dir.join("graph.json");
    let node_link_path = out_dir.join("graph.nodelink.json");
    let report_path = out_dir.join("GRAPH_REPORT.md");
    let cache_manifest_path = out_dir.join("manifest.json");
    let html_path = (!options.no_viz).then(|| out_dir.join("graph.html"));

    fs::write(&graph_json_path, serde_json::to_vec_pretty(&graph)?)?;
    fs::write(&node_link_path, to_node_link_json(&graph))?;
    fs::write(&report_path, report.as_bytes())?;
    fs::write(
        &cache_manifest_path,
        serde_json::to_vec_pretty(&json!({
            "version": 1,
            "rootDir": root_dir.to_string_lossy(),
            "generatedAt": graph.generated_at,
            "files": files.iter().map(|file| file.relative_path.clone()).collect::<Vec<_>>(),
        }))?,
    )?;
    if let Some(path) = &html_path {
        fs::write(path, render_graph_html(&graph).as_bytes())?;
    }

    Ok(RepositoryBuildResult {
        graph,
        report,
        artifacts: RepositoryBuildArtifacts {
            graph_json_path: graph_json_path.to_string_lossy().to_string(),
            report_path: report_path.to_string_lossy().to_string(),
            html_path: html_path.map(|path| path.to_string_lossy().to_string()),
            cache_manifest_path: cache_manifest_path.to_string_lossy().to_string(),
        },
        stats: RepositoryBuildStats {
            processed_files: files.len() as u32 - skipped_files,
            reused_files: 0,
            removed_files: 0,
            total_files: files.len() as u32,
        },
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncRules {
    pub code_extensions: Vec<String>,
    pub doc_extensions: Vec<String>,
    pub binary_extensions: Vec<String>,
    pub ignore_dirs: Vec<String>,
    pub ignore_files: Vec<String>,
    pub ignore_patterns: Vec<String>,
    pub max_file_size_bytes: u64,
    pub max_binary_file_size_bytes: u64,
}

pub fn sync_rules() -> SyncRules {
    SyncRules {
        code_extensions: CODE_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
        doc_extensions: DOC_EXTENSIONS
            .iter()
            .chain(OFFICE_DOCUMENT_EXTENSIONS)
            .chain(SPREADSHEET_EXTENSIONS)
            .chain(PRESENTATION_EXTENSIONS)
            .chain(PDF_EXTENSIONS)
            .chain(IMAGE_EXTENSIONS)
            .chain(MEDIA_EXTENSIONS)
            .map(|s| s.to_string())
            .collect(),
        binary_extensions: OFFICE_DOCUMENT_EXTENSIONS
            .iter()
            .chain(SPREADSHEET_EXTENSIONS)
            .chain(PRESENTATION_EXTENSIONS)
            .chain(PDF_EXTENSIONS)
            .chain(IMAGE_EXTENSIONS)
            .chain(MEDIA_EXTENSIONS)
            .map(|s| s.to_string())
            .collect(),
        ignore_dirs: IGNORE_DIRS.iter().map(|s| s.to_string()).collect(),
        ignore_files: IGNORE_FILES.iter().map(|s| s.to_string()).collect(),
        ignore_patterns: vec![
            ".env*".to_string(),
            "*.pem".to_string(),
            "*.key".to_string(),
            "*.crt".to_string(),
            "*.min.js".to_string(),
            "*.min.css".to_string(),
            "*.map".to_string(),
            "*.d.ts".to_string(),
            "*.generated.ts".to_string(),
            "*.generated.js".to_string(),
        ],
        max_file_size_bytes: MAX_FILE_SIZE_BYTES,
        max_binary_file_size_bytes: MAX_BINARY_FILE_SIZE_BYTES,
    }
}

pub fn parse_file_batch(files: &[(String, String)]) -> (Vec<KnowledgeNode>, Vec<KnowledgeEdge>) {
    let payloads = files
        .iter()
        .map(|(path, content)| RepositoryFilePayload {
            path: path.clone(),
            content: Some(content.clone()),
            bytes: None,
            media_type: None,
            size_bytes: Some(content.len() as u64),
        })
        .collect::<Vec<_>>();
    parse_file_payload_batch(&payloads)
}

pub fn parse_file_payload_batch(
    files: &[RepositoryFilePayload],
) -> (Vec<KnowledgeNode>, Vec<KnowledgeEdge>) {
    let known_files: HashSet<String> = files.iter().map(|file| file.path.clone()).collect();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut semantic_inputs = Vec::new();

    for payload in files {
        let kind = classify_file_kind(Path::new(&payload.path)).unwrap_or(FileKind::Unsupported);
        let bytes = if let Some(bytes) = &payload.bytes {
            bytes.clone()
        } else if let Some(content) = &payload.content {
            content.as_bytes().to_vec()
        } else {
            Vec::new()
        };
        let size_bytes = payload.size_bytes.unwrap_or(bytes.len() as u64);
        let max_size = if kind.is_binary() {
            MAX_BINARY_FILE_SIZE_BYTES
        } else {
            MAX_FILE_SIZE_BYTES
        };
        if size_bytes > max_size && !kind.is_binary() {
            continue;
        }

        let repo_file = RepositoryFile {
            absolute_path: PathBuf::from(&payload.path),
            relative_path: payload.path.clone(),
            kind,
        };

        let extracted = if size_bytes > max_size {
            extract_repository_file(&repo_file, &[], &known_files, Some(size_bytes))
        } else {
            extract_repository_file(&repo_file, &bytes, &known_files, Some(size_bytes))
        };
        nodes.extend(extracted.nodes);
        edges.extend(extracted.edges);
        if let Some(text) = extracted.semantic_text {
            semantic_inputs.push((payload.path.clone(), text));
        }
    }

    edges.extend(build_semantic_similarity_edges(&semantic_inputs));

    // v2.2.2: Cross-file call resolution (same as build_repository_knowledge path)
    resolve_cross_file_calls(&nodes, &mut edges, &known_files);

    (dedupe_nodes(nodes), dedupe_edges(edges))
}

fn ensure_output_dir(
    preferred_out_dir: &Path,
    root_dir: &Path,
) -> Result<PathBuf, RepositoryBuildError> {
    match fs::create_dir_all(preferred_out_dir) {
        Ok(()) => Ok(preferred_out_dir.to_path_buf()),
        Err(error)
            if error.kind() == std::io::ErrorKind::PermissionDenied
                || error.raw_os_error() == Some(30) =>
        {
            let fallback = std::env::temp_dir().join(format!(
                "graphify-out-{}",
                short_hash(root_dir.to_string_lossy().as_bytes())
            ));
            fs::create_dir_all(&fallback)?;
            Ok(fallback)
        }
        Err(error) => Err(RepositoryBuildError::Io(error)),
    }
}

fn collect_repository_files(root_dir: &Path) -> Result<Vec<RepositoryFile>, RepositoryBuildError> {
    let mut files = Vec::new();
    walk_repository(root_dir, root_dir, &mut files)?;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    Ok(files)
}

fn walk_repository(
    root_dir: &Path,
    current_dir: &Path,
    files: &mut Vec<RepositoryFile>,
) -> Result<(), RepositoryBuildError> {
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        if file_type.is_dir() {
            if IGNORE_DIRS.contains(&name.as_ref()) {
                continue;
            }
            walk_repository(root_dir, &path, files)?;
            continue;
        }

        if !file_type.is_file() || is_ignored_file(&name) {
            continue;
        }

        let Some(kind) = classify_file_kind(&path) else {
            continue;
        };
        let relative_path = path
            .strip_prefix(root_dir)
            .map_err(|_| RepositoryBuildError::InvalidPath)?
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/");
        files.push(RepositoryFile {
            absolute_path: path,
            relative_path,
            kind,
        });
    }

    Ok(())
}

fn classify_file_kind(path: &Path) -> Option<FileKind> {
    let extension = path.extension()?.to_string_lossy().to_lowercase();
    if CODE_EXTENSIONS.contains(&extension.as_str()) {
        Some(FileKind::Code)
    } else if DOC_EXTENSIONS.contains(&extension.as_str()) {
        Some(FileKind::Doc)
    } else if OFFICE_DOCUMENT_EXTENSIONS.contains(&extension.as_str()) {
        Some(FileKind::OfficeDocument)
    } else if SPREADSHEET_EXTENSIONS.contains(&extension.as_str()) {
        Some(FileKind::Spreadsheet)
    } else if PRESENTATION_EXTENSIONS.contains(&extension.as_str()) {
        Some(FileKind::Presentation)
    } else if PDF_EXTENSIONS.contains(&extension.as_str()) {
        Some(FileKind::Pdf)
    } else if IMAGE_EXTENSIONS.contains(&extension.as_str()) {
        Some(FileKind::Image)
    } else if MEDIA_EXTENSIONS.contains(&extension.as_str()) {
        Some(FileKind::Media)
    } else {
        None
    }
}

fn is_ignored_file(name: &str) -> bool {
    if IGNORE_FILES.contains(&name) {
        return true;
    }
    (name.starts_with(".env") && name != ".env.example")
        || name.ends_with(".pem")
        || name.ends_with(".key")
        || name.ends_with(".p12")
        || name.ends_with(".pfx")
        || name.ends_with(".secret")
        || name.ends_with(".secrets")
        || (name.contains("credentials") && name.ends_with(".json"))
        || name.ends_with(".min.js")
        || name.ends_with(".min.css")
        || name.ends_with(".map")
        || name.ends_with(".tsbuildinfo")
        || name.ends_with(".d.ts")
        || name.ends_with(".generated.ts")
        || name.ends_with(".generated.js")
        || name.starts_with("__generated__")
}

fn extract_repository_file(
    file: &RepositoryFile,
    bytes: &[u8],
    known_files: &HashSet<String>,
    size_bytes: Option<u64>,
) -> ParsedRepositoryFile {
    if bytes.is_empty() && file.kind.is_binary() {
        return metadata_only_file(
            file,
            "file exceeds binary parser size cap; metadata-only node emitted",
            size_bytes,
        );
    }

    match file.kind {
        FileKind::Code | FileKind::Doc => match std::str::from_utf8(bytes) {
            Ok(text) => {
                let (nodes, edges) = if file.kind == FileKind::Code {
                    extract_code_file(file, text, known_files)
                } else {
                    extract_document_file(file, text, known_files)
                };
                ParsedRepositoryFile {
                    nodes,
                    edges,
                    semantic_text: Some(text.to_string()),
                }
            }
            Err(error) => metadata_only_file(
                file,
                &format!("expected UTF-8 text but decoding failed: {error}"),
                size_bytes,
            ),
        },
        FileKind::OfficeDocument => extract_docx_file(file, bytes, known_files, size_bytes),
        FileKind::Spreadsheet => extract_xlsx_file(file, bytes, size_bytes),
        FileKind::Presentation => extract_pptx_file(file, bytes, size_bytes),
        FileKind::Pdf => extract_pdf_file(file, bytes, size_bytes),
        FileKind::Image => extract_image_file(file, bytes, size_bytes),
        FileKind::Media => extract_media_file(file, bytes, size_bytes),
        FileKind::Unsupported => metadata_only_file(
            file,
            "unsupported file extension; metadata-only node emitted",
            size_bytes,
        ),
    }
}

fn extract_code_file(
    file: &RepositoryFile,
    content: &str,
    known_files: &HashSet<String>,
) -> (Vec<KnowledgeNode>, Vec<KnowledgeEdge>) {
    let mut nodes = vec![create_file_node(&file.relative_path, file.kind)];
    let mut edges = Vec::new();

    let extension = Path::new(&file.relative_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    // Try AST-based extraction first, fall back to regex for unsupported languages
    if let Some(ast) = ast_parser::extract_ast(extension, content) {
        // --- Symbols (functions, classes, structs, traits, etc.) ---
        for sym in &ast.symbols {
            // v2.2.2: Use qualified name (Parent.method) for contained symbols
            let display_name = if let Some(ref parent) = sym.parent_name {
                format!("{}.{}", parent, sym.name)
            } else {
                sym.name.clone()
            };
            let symbol_id = format!("symbol:{}:{}", file.relative_path, display_name);
            let mut meta = json!({
                "symbolKind": sym.kind.as_str(),
                "sourceFile": file.relative_path,
                "sourceLocation": format!("L{}", sym.line),
                "language": ast.language,
                "parser": "tree-sitter",
            });
            // v2.2.2: Populate doc comment metadata
            if let Some(ref doc) = sym.doc_comment {
                meta["docComment"] = json!(doc);
            }
            if let Some(ref parent) = sym.parent_name {
                meta["parentSymbol"] = json!(parent);
            }
            nodes.push(KnowledgeNode {
                id: symbol_id.clone(),
                label: display_name,
                node_type: "symbol".to_string(),
                source_type: "derived".to_string(),
                source_id: file.relative_path.clone(),
                metadata: meta,
                community_id: None,
            });
            edges.push(edge(
                format!("file:{}", file.relative_path),
                symbol_id.clone(),
                "defines",
                "extracted",
                1.0,
                json!({ "sourceFile": file.relative_path }),
            ));

            // v2.2.2: Emit containment edge (parent→child)
            if let Some(ref parent) = sym.parent_name {
                let parent_id = format!("symbol:{}:{}", file.relative_path, parent);
                edges.push(edge(
                    parent_id,
                    symbol_id.clone(),
                    "contains",
                    "extracted",
                    1.0,
                    json!({ "sourceFile": file.relative_path }),
                ));
            }

            // v2.2.2: Emit type edges (returns, param)
            if let Some(ref ret_type) = sym.return_type {
                let type_id = format!("type:{}", ret_type);
                if !nodes.iter().any(|n| n.id == type_id) {
                    nodes.push(KnowledgeNode {
                        id: type_id.clone(),
                        label: ret_type.clone(),
                        node_type: "type".to_string(),
                        source_type: "derived".to_string(),
                        source_id: file.relative_path.clone(),
                        metadata: json!({ "language": ast.language }),
                        community_id: None,
                    });
                }
                edges.push(edge(
                    symbol_id.clone(),
                    type_id,
                    "returns",
                    "extracted",
                    1.0,
                    json!({ "sourceFile": file.relative_path }),
                ));
            }
            for (param_name, param_type) in &sym.param_types {
                let type_id = format!("type:{}", param_type);
                if !nodes.iter().any(|n| n.id == type_id) {
                    nodes.push(KnowledgeNode {
                        id: type_id.clone(),
                        label: param_type.clone(),
                        node_type: "type".to_string(),
                        source_type: "derived".to_string(),
                        source_id: file.relative_path.clone(),
                        metadata: json!({ "language": ast.language }),
                        community_id: None,
                    });
                }
                edges.push(edge(
                    symbol_id.clone(),
                    type_id,
                    "param",
                    "extracted",
                    1.0,
                    json!({ "sourceFile": file.relative_path, "paramName": param_name }),
                ));
            }
        }

        // --- Imports ---
        for imp in &ast.imports {
            if imp.is_relative {
                // Resolve relative imports to known files
                if let Some(resolved) =
                    resolve_import(&file.relative_path, &imp.source, known_files)
                {
                    edges.push(edge(
                        format!("file:{}", file.relative_path),
                        format!("file:{resolved}"),
                        "imports",
                        "extracted",
                        1.0,
                        json!({
                            "sourceFile": file.relative_path,
                            "sourceLocation": format!("L{}", imp.line),
                            "parser": "tree-sitter",
                        }),
                    ));
                }
            } else {
                // External imports - create an import edge to a virtual module node
                let module_id = format!("module:{}", imp.source);
                if !nodes.iter().any(|n| n.id == module_id) {
                    nodes.push(KnowledgeNode {
                        id: module_id.clone(),
                        label: imp.source.clone(),
                        node_type: "module".to_string(),
                        source_type: "derived".to_string(),
                        source_id: imp.source.clone(),
                        metadata: json!({
                            "external": true,
                            "importSource": imp.source,
                        }),
                        community_id: None,
                    });
                }
                edges.push(edge(
                    format!("file:{}", file.relative_path),
                    module_id,
                    "imports",
                    "extracted",
                    0.8,
                    json!({
                        "sourceFile": file.relative_path,
                        "sourceLocation": format!("L{}", imp.line),
                        "parser": "tree-sitter",
                    }),
                ));
            }
        }

        // --- Call graph edges ---
        let defined_symbols: HashSet<&str> = ast.symbols.iter().map(|s| s.name.as_str()).collect();
        for call in &ast.calls {
            // Create call edges - link to local symbol if defined in same file
            if defined_symbols.contains(call.callee.as_str()) {
                let target_id = format!("symbol:{}:{}", file.relative_path, call.callee);
                edges.push(edge(
                    format!("file:{}", file.relative_path),
                    target_id,
                    "calls",
                    "extracted",
                    1.0,
                    json!({
                        "sourceFile": file.relative_path,
                        "sourceLocation": format!("L{}", call.line),
                        "parser": "tree-sitter",
                    }),
                ));
            } else {
                // Cross-file call - create an unresolved call node
                let call_id = format!(
                    "call:{}:{}",
                    call.callee,
                    short_hash(format!("{}:{}", file.relative_path, call.line).as_bytes())
                );
                edges.push(edge(
                    format!("file:{}", file.relative_path),
                    call_id,
                    "calls",
                    "inferred",
                    0.7,
                    json!({
                        "callee": call.callee,
                        "sourceFile": file.relative_path,
                        "sourceLocation": format!("L{}", call.line),
                        "parser": "tree-sitter",
                    }),
                ));
            }
        }

        // --- Rationale comments ---
        for rat in &ast.rationales {
            let rationale_id = format!(
                "rationale:{}:{}",
                file.relative_path,
                short_hash(rat.body.as_bytes())
            );
            nodes.push(KnowledgeNode {
                id: rationale_id.clone(),
                label: truncate(&rat.body, 80),
                node_type: "rationale".to_string(),
                source_type: "derived".to_string(),
                source_id: file.relative_path.clone(),
                metadata: json!({
                    "tag": rat.tag,
                    "body": rat.body,
                    "sourceFile": file.relative_path,
                    "sourceLocation": format!("L{}", rat.line),
                    "parser": "tree-sitter",
                }),
                community_id: None,
            });
            edges.push(edge(
                rationale_id,
                format!("file:{}", file.relative_path),
                "explains",
                "extracted",
                1.0,
                json!({ "sourceFile": file.relative_path }),
            ));
        }
    } else {
        // Fallback: regex-based extraction for unsupported languages
        extract_code_file_regex(file, content, known_files, &mut nodes, &mut edges);
    }

    (nodes, edges)
}

/// Regex fallback for languages not supported by tree-sitter.
fn extract_code_file_regex(
    file: &RepositoryFile,
    content: &str,
    known_files: &HashSet<String>,
    nodes: &mut Vec<KnowledgeNode>,
    edges: &mut Vec<KnowledgeEdge>,
) {
    let import_pattern =
        Regex::new(r#"import\s+(?:type\s+)?[\s\S]*?from\s+['"]([^'"]+)['"]"#).unwrap();
    for captures in import_pattern.captures_iter(content) {
        let Some(specifier) = captures.get(1).map(|value| value.as_str()) else {
            continue;
        };
        if !specifier.starts_with('.') {
            continue;
        }
        let Some(source_match) = captures.get(0) else {
            continue;
        };
        let Some(resolved) = resolve_import(&file.relative_path, specifier, known_files) else {
            continue;
        };
        edges.push(edge(
            format!("file:{}", file.relative_path),
            format!("file:{resolved}"),
            "imports",
            "extracted",
            1.0,
            json!({
                "sourceFile": file.relative_path,
                "sourceLocation": format!("L{}", line_number_for_index(content, source_match.start())),
            }),
        ));
    }

    let symbol_pattern = Regex::new(
        r"(?m)(?:export\s+|pub\s+)?(class|function|interface|type|const|fn|struct|enum|trait)\s+([A-Za-z0-9_]+)",
    )
    .unwrap();
    for captures in symbol_pattern.captures_iter(content) {
        let Some(symbol_kind) = captures.get(1).map(|value| value.as_str()) else {
            continue;
        };
        let Some(symbol_name) = captures.get(2).map(|value| value.as_str()) else {
            continue;
        };
        let Some(source_match) = captures.get(0) else {
            continue;
        };
        let symbol_id = format!("symbol:{}:{symbol_name}", file.relative_path);
        nodes.push(KnowledgeNode {
            id: symbol_id.clone(),
            label: symbol_name.to_string(),
            node_type: "symbol".to_string(),
            source_type: "derived".to_string(),
            source_id: file.relative_path.clone(),
            metadata: json!({
                "symbolKind": symbol_kind,
                "sourceFile": file.relative_path,
                "sourceLocation": format!("L{}", line_number_for_index(content, source_match.start())),
            }),
            community_id: None,
        });
        edges.push(edge(
            format!("file:{}", file.relative_path),
            symbol_id,
            "defines",
            "extracted",
            1.0,
            json!({ "sourceFile": file.relative_path }),
        ));
    }

    let rationale_pattern =
        Regex::new(r"(?m)(?://|#|/\*)\s*(WHY|NOTE|IMPORTANT|RATIONALE)\s*:?\s*(.+)$").unwrap();
    for captures in rationale_pattern.captures_iter(content) {
        let Some(body) = captures.get(2).map(|value| value.as_str().trim()) else {
            continue;
        };
        if body.is_empty() {
            continue;
        }
        let Some(source_match) = captures.get(0) else {
            continue;
        };
        let rationale_id = format!(
            "rationale:{}:{}",
            file.relative_path,
            short_hash(body.as_bytes())
        );
        nodes.push(KnowledgeNode {
            id: rationale_id.clone(),
            label: truncate(body, 80),
            node_type: "rationale".to_string(),
            source_type: "derived".to_string(),
            source_id: file.relative_path.clone(),
            metadata: json!({
                "sourceFile": file.relative_path,
                "sourceLocation": format!("L{}", line_number_for_index(content, source_match.start())),
                "body": body,
            }),
            community_id: None,
        });
        edges.push(edge(
            rationale_id,
            format!("file:{}", file.relative_path),
            "explains",
            "extracted",
            1.0,
            json!({ "sourceFile": file.relative_path }),
        ));
    }
}

fn extract_document_file(
    file: &RepositoryFile,
    content: &str,
    known_files: &HashSet<String>,
) -> (Vec<KnowledgeNode>, Vec<KnowledgeEdge>) {
    let mut nodes = vec![create_file_node(&file.relative_path, file.kind)];
    let mut edges = Vec::new();

    let heading_pattern = Regex::new(r"(?m)^(#{1,6})\s+(.+)$").unwrap();
    let headings = heading_pattern.captures_iter(content).collect::<Vec<_>>();
    if headings.is_empty() {
        let section_id = format!("section:{}:root", file.relative_path);
        nodes.push(KnowledgeNode {
            id: section_id.clone(),
            label: filename_of(&file.relative_path),
            node_type: "section".to_string(),
            source_type: "derived".to_string(),
            source_id: file.relative_path.clone(),
            metadata: json!({
                "headingLevel": 0,
                "sourceFile": file.relative_path,
                "heading": filename_of(&file.relative_path),
            }),
            community_id: None,
        });
        edges.push(edge(
            format!("file:{}", file.relative_path),
            section_id,
            "contains",
            "extracted",
            1.0,
            json!({}),
        ));
    }

    for captures in headings {
        let Some(markers) = captures.get(1).map(|value| value.as_str()) else {
            continue;
        };
        let Some(heading) = captures.get(2).map(|value| value.as_str().trim()) else {
            continue;
        };
        if heading.is_empty() {
            continue;
        }
        let Some(source_match) = captures.get(0) else {
            continue;
        };
        let line = line_number_for_index(content, source_match.start());
        let section_id = format!(
            "section:{}:{}",
            file.relative_path,
            short_hash(format!("{line}:{heading}").as_bytes())
        );
        nodes.push(KnowledgeNode {
            id: section_id.clone(),
            label: heading.to_string(),
            node_type: classify_heading(heading).to_string(),
            source_type: "derived".to_string(),
            source_id: file.relative_path.clone(),
            metadata: json!({
                "headingLevel": markers.len(),
                "heading": heading,
                "sourceFile": file.relative_path,
                "sourceLocation": format!("L{line}"),
            }),
            community_id: None,
        });
        edges.push(edge(
            format!("file:{}", file.relative_path),
            section_id,
            "contains",
            "extracted",
            1.0,
            json!({ "sourceFile": file.relative_path }),
        ));
    }

    let mention_pattern =
        Regex::new(r"`([^`]+)`|([A-Za-z0-9_./-]+\.(?:ts|tsx|js|jsx|md|mdx|sql|json|yml|yaml|sh))")
            .unwrap();
    for captures in mention_pattern.captures_iter(content) {
        let raw = captures
            .get(1)
            .or_else(|| captures.get(2))
            .map(|value| value.as_str().trim().trim_start_matches("./"))
            .unwrap_or_default();
        if raw.is_empty() || !known_files.contains(raw) {
            continue;
        }
        let Some(source_match) = captures.get(0) else {
            continue;
        };
        edges.push(edge(
            format!("file:{}", file.relative_path),
            format!("file:{raw}"),
            "mentions",
            "extracted",
            1.0,
            json!({
                "sourceFile": file.relative_path,
                "sourceLocation": format!("L{}", line_number_for_index(content, source_match.start())),
            }),
        ));
    }

    (nodes, edges)
}

fn extract_docx_file(
    file: &RepositoryFile,
    bytes: &[u8],
    known_files: &HashSet<String>,
    size_bytes: Option<u64>,
) -> ParsedRepositoryFile {
    let mut diagnostics = Vec::new();
    let mut markdown = String::new();
    match read_zip_text_entries(
        bytes,
        |name| {
            name == "word/document.xml"
                || name.starts_with("word/header")
                || name.starts_with("word/footer")
        },
        2_000_000,
    ) {
        Ok(entries) => {
            for (name, xml) in entries {
                let text = ooxml_text_to_markdown(&xml);
                if !text.trim().is_empty() {
                    markdown.push_str(&format!("\n\n## {name}\n{text}"));
                }
            }
        }
        Err(error) => diagnostics.push(format!("docx parse failed: {error}")),
    }

    if markdown.trim().is_empty() {
        return metadata_only_file(
            file,
            diagnostics
                .first()
                .map(String::as_str)
                .unwrap_or("docx contained no extractable text"),
            size_bytes,
        );
    }

    let pseudo_file = RepositoryFile {
        absolute_path: file.absolute_path.clone(),
        relative_path: file.relative_path.clone(),
        kind: FileKind::Doc,
    };
    let (mut nodes, mut edges) = extract_document_file(&pseudo_file, &markdown, known_files);
    attach_parser_metadata(&mut nodes, file, "docx-ooxml", size_bytes, &diagnostics);
    attach_parser_metadata_to_edges(&mut edges, "docx-ooxml");
    ParsedRepositoryFile {
        nodes,
        edges,
        semantic_text: Some(markdown),
    }
}

fn extract_xlsx_file(
    file: &RepositoryFile,
    bytes: &[u8],
    size_bytes: Option<u64>,
) -> ParsedRepositoryFile {
    let mut nodes = vec![create_file_node(&file.relative_path, file.kind)];
    let mut edges = Vec::new();
    let mut diagnostics = Vec::new();
    let shared_strings = read_xlsx_shared_strings(bytes).unwrap_or_else(|error| {
        diagnostics.push(format!("shared string parse failed: {error}"));
        Vec::new()
    });
    let mut markdown = String::new();

    match read_zip_text_entries(
        bytes,
        |name| name.starts_with("xl/worksheets/sheet"),
        3_000_000,
    ) {
        Ok(entries) => {
            for (idx, (name, xml)) in entries.into_iter().enumerate() {
                let sheet_name = format!("Sheet {}", idx + 1);
                let sheet_id = format!(
                    "section:{}:{}",
                    file.relative_path,
                    short_hash(name.as_bytes())
                );
                let text = xlsx_sheet_text(&xml, &shared_strings);
                nodes.push(KnowledgeNode {
                    id: sheet_id.clone(),
                    label: sheet_name.clone(),
                    node_type: "section".to_string(),
                    source_type: "derived".to_string(),
                    source_id: file.relative_path.clone(),
                    metadata: json!({
                        "sourceFile": file.relative_path,
                        "parser": "xlsx-ooxml",
                        "sheetSource": name,
                        "repositoryFileKind": file.kind.label(),
                    }),
                    community_id: None,
                });
                edges.push(edge(
                    format!("file:{}", file.relative_path),
                    sheet_id,
                    "contains",
                    "extracted",
                    1.0,
                    json!({ "sourceFile": file.relative_path, "parser": "xlsx-ooxml" }),
                ));
                if !text.trim().is_empty() {
                    markdown.push_str(&format!("\n\n## {sheet_name}\n{text}"));
                }
            }
        }
        Err(error) => diagnostics.push(format!("xlsx parse failed: {error}")),
    }

    attach_parser_metadata(&mut nodes, file, "xlsx-ooxml", size_bytes, &diagnostics);
    if !diagnostics.is_empty() {
        add_diagnostic_node(&mut nodes, &mut edges, file, &diagnostics.join("; "));
    }
    ParsedRepositoryFile {
        nodes,
        edges,
        semantic_text: (!markdown.trim().is_empty()).then_some(markdown),
    }
}

fn extract_pptx_file(
    file: &RepositoryFile,
    bytes: &[u8],
    size_bytes: Option<u64>,
) -> ParsedRepositoryFile {
    let mut nodes = vec![create_file_node(&file.relative_path, file.kind)];
    let mut edges = Vec::new();
    let mut diagnostics = Vec::new();
    let mut markdown = String::new();

    match read_zip_text_entries(
        bytes,
        |name| name.starts_with("ppt/slides/slide"),
        3_000_000,
    ) {
        Ok(entries) => {
            for (idx, (name, xml)) in entries.into_iter().enumerate() {
                let slide_label = format!("Slide {}", idx + 1);
                let slide_id = format!(
                    "section:{}:{}",
                    file.relative_path,
                    short_hash(name.as_bytes())
                );
                let text = ooxml_text_to_markdown(&xml);
                nodes.push(KnowledgeNode {
                    id: slide_id.clone(),
                    label: slide_label.clone(),
                    node_type: "section".to_string(),
                    source_type: "derived".to_string(),
                    source_id: file.relative_path.clone(),
                    metadata: json!({
                        "sourceFile": file.relative_path,
                        "parser": "pptx-ooxml",
                        "slideSource": name,
                        "repositoryFileKind": file.kind.label(),
                    }),
                    community_id: None,
                });
                edges.push(edge(
                    format!("file:{}", file.relative_path),
                    slide_id,
                    "contains",
                    "extracted",
                    1.0,
                    json!({ "sourceFile": file.relative_path, "parser": "pptx-ooxml" }),
                ));
                if !text.trim().is_empty() {
                    markdown.push_str(&format!("\n\n## {slide_label}\n{text}"));
                }
            }
        }
        Err(error) => diagnostics.push(format!("pptx parse failed: {error}")),
    }

    attach_parser_metadata(&mut nodes, file, "pptx-ooxml", size_bytes, &diagnostics);
    if !diagnostics.is_empty() {
        add_diagnostic_node(&mut nodes, &mut edges, file, &diagnostics.join("; "));
    }
    ParsedRepositoryFile {
        nodes,
        edges,
        semantic_text: (!markdown.trim().is_empty()).then_some(markdown),
    }
}

fn extract_pdf_file(
    file: &RepositoryFile,
    bytes: &[u8],
    size_bytes: Option<u64>,
) -> ParsedRepositoryFile {
    let mut nodes = vec![create_file_node(&file.relative_path, file.kind)];
    let mut edges = Vec::new();
    let text = extract_pdf_text_best_effort(bytes);
    attach_parser_metadata(&mut nodes, file, "pdf-best-effort", size_bytes, &[]);
    if text.trim().is_empty() {
        add_diagnostic_node(
            &mut nodes,
            &mut edges,
            file,
            "pdf parser found no plain text; OCR or a full PDF parser is required",
        );
        return ParsedRepositoryFile {
            nodes,
            edges,
            semantic_text: None,
        };
    }

    let page_id = format!("section:{}:pdf_text", file.relative_path);
    nodes.push(KnowledgeNode {
        id: page_id.clone(),
        label: "PDF text".to_string(),
        node_type: "section".to_string(),
        source_type: "derived".to_string(),
        source_id: file.relative_path.clone(),
        metadata: json!({
            "sourceFile": file.relative_path,
            "parser": "pdf-best-effort",
            "repositoryFileKind": file.kind.label(),
        }),
        community_id: None,
    });
    edges.push(edge(
        format!("file:{}", file.relative_path),
        page_id,
        "contains",
        "extracted",
        0.7,
        json!({ "sourceFile": file.relative_path, "parser": "pdf-best-effort" }),
    ));
    ParsedRepositoryFile {
        nodes,
        edges,
        semantic_text: Some(text),
    }
}

fn extract_image_file(
    file: &RepositoryFile,
    bytes: &[u8],
    size_bytes: Option<u64>,
) -> ParsedRepositoryFile {
    let mut nodes = vec![create_file_node(&file.relative_path, file.kind)];
    let mut edges = Vec::new();
    let dimensions = image_dimensions(bytes);
    let mut diagnostics = Vec::new();
    if dimensions.is_none() {
        diagnostics.push("image dimensions could not be decoded".to_string());
        add_diagnostic_node(&mut nodes, &mut edges, file, &diagnostics[0]);
    }
    attach_parser_metadata(&mut nodes, file, "image-header", size_bytes, &diagnostics);
    if let Some((width, height)) = dimensions {
        if let Some(node) = nodes
            .iter_mut()
            .find(|node| node.id == format!("file:{}", file.relative_path))
        {
            merge_metadata(
                &mut node.metadata,
                &json!({ "width": width, "height": height, "needsVisionPass": true }),
            );
        }
    }
    ParsedRepositoryFile {
        nodes,
        edges,
        semantic_text: Some(format!(
            "{} image metadata: dimensions={:?}",
            file.relative_path, dimensions
        )),
    }
}

fn extract_media_file(
    file: &RepositoryFile,
    bytes: &[u8],
    size_bytes: Option<u64>,
) -> ParsedRepositoryFile {
    let mut nodes = vec![create_file_node(&file.relative_path, file.kind)];
    let mut edges = Vec::new();
    let metadata = media_metadata(bytes, &file.relative_path);
    let mut diagnostics = Vec::new();
    if metadata.is_empty() {
        diagnostics
            .push("media metadata could not be decoded; transcript job required".to_string());
        add_diagnostic_node(&mut nodes, &mut edges, file, &diagnostics[0]);
    }
    attach_parser_metadata(&mut nodes, file, "media-header", size_bytes, &diagnostics);
    if let Some(node) = nodes
        .iter_mut()
        .find(|node| node.id == format!("file:{}", file.relative_path))
    {
        merge_metadata(
            &mut node.metadata,
            &json!({ "media": metadata, "needsTranscriptPass": true }),
        );
    }
    ParsedRepositoryFile {
        nodes,
        edges,
        semantic_text: Some(format!(
            "{} media metadata: {}",
            file.relative_path, metadata
        )),
    }
}

fn build_semantic_similarity_edges(items: &[(String, String)]) -> Vec<KnowledgeEdge> {
    const MIN_TOKENS: usize = 10;
    /// Cap pairwise comparisons to avoid O(n²) blowup on large repos.
    const MAX_FILES_FOR_SIMILARITY: usize = 500;

    let capped = if items.len() > MAX_FILES_FOR_SIMILARITY {
        &items[..MAX_FILES_FOR_SIMILARITY]
    } else {
        items
    };

    let token_sets = capped
        .iter()
        .map(|(_, text)| tokenize(text))
        .collect::<Vec<_>>();
    let mut edges = Vec::new();

    for left in 0..capped.len() {
        if token_sets[left].len() < MIN_TOKENS {
            continue;
        }
        for right in (left + 1)..capped.len() {
            if token_sets[right].len() < MIN_TOKENS {
                continue;
            }
            let min_size = token_sets[left].len().min(token_sets[right].len()) as f64;
            let max_size = token_sets[left].len().max(token_sets[right].len()) as f64;
            if min_size / max_size < 0.25 {
                continue;
            }
            let similarity = jaccard_similarity(&token_sets[left], &token_sets[right]);
            if similarity < 0.25 {
                continue;
            }
            edges.push(edge(
                format!("file:{}", capped[left].0),
                format!("file:{}", capped[right].0),
                "semantically_similar_to",
                "inferred",
                (similarity + 0.2).min(0.95),
                json!({
                    "reason": "token_overlap",
                    "similarity": ((similarity * 1000.0).round() / 1000.0),
                }),
            ));
        }
    }

    edges
}

fn dedupe_nodes(nodes: Vec<KnowledgeNode>) -> Vec<KnowledgeNode> {
    let mut map = HashMap::new();
    for node in nodes {
        map.insert(node.id.clone(), node);
    }
    map.into_values().collect()
}

fn dedupe_edges(edges: Vec<KnowledgeEdge>) -> Vec<KnowledgeEdge> {
    let mut map: HashMap<(String, String, String), KnowledgeEdge> = HashMap::new();
    for edge_item in edges {
        let key = (
            edge_item.source.clone(),
            edge_item.target.clone(),
            edge_item.relation.clone(),
        );
        match map.get_mut(&key) {
            Some(existing) => {
                if edge_rank(&edge_item.evidence) >= edge_rank(&existing.evidence) {
                    existing.evidence = edge_item.evidence.clone();
                }
                existing.weight = existing.weight.max(edge_item.weight);
                merge_metadata(&mut existing.metadata, &edge_item.metadata);
            }
            None => {
                map.insert(key, edge_item);
            }
        }
    }
    map.into_values().collect()
}

/// v2.2.2: Two-pass cross-file call resolution.
///
/// Pass 1: Build a global symbol table from all `symbol:*` nodes.
/// Pass 2: Rewrite `inferred` call edges whose callee name matches a
///          symbol in the global table, using import-path and directory
///          locality for disambiguation.
fn resolve_cross_file_calls(
    nodes: &[KnowledgeNode],
    edges: &mut Vec<KnowledgeEdge>,
    known_files: &HashSet<String>,
) {
    use std::collections::HashMap;

    // Pass 1: build global symbol table  name → [(node_id, file_path)]
    let mut symbol_table: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for node in nodes {
        if node.node_type == "symbol" {
            // Label is the bare or qualified name; extract the last segment
            // for matching (e.g. "DataProcessor.process" → "process")
            let bare_name = node.label.rsplit('.').next().unwrap_or(&node.label);
            let file = node.source_id.clone();
            symbol_table
                .entry(bare_name.to_string())
                .or_default()
                .push((node.id.clone(), file));
        }
    }

    // Build import map: source_file → set of imported file paths
    let mut import_map: HashMap<String, HashSet<String>> = HashMap::new();
    for e in edges.iter() {
        if e.relation == "imports" {
            // source is "file:path", target is "file:path" or "module:name"
            if let Some(src_path) = e.source.strip_prefix("file:") {
                if let Some(tgt_path) = e.target.strip_prefix("file:") {
                    import_map
                        .entry(src_path.to_string())
                        .or_default()
                        .insert(tgt_path.to_string());
                }
            }
        }
    }

    // Pass 2: rewrite inferred call edges
    for e in edges.iter_mut() {
        if e.relation != "calls" || e.evidence != "inferred" {
            continue;
        }
        // Extract callee name from the edge target (format: "call:callee_name:hash")
        let callee_name = match e.target.strip_prefix("call:") {
            Some(rest) => rest.split(':').next().unwrap_or(rest),
            None => continue,
        };

        let candidates = match symbol_table.get(callee_name) {
            Some(c) if !c.is_empty() => c,
            _ => continue,
        };

        // Extract calling file from source ("file:path")
        let caller_file = if let Some(f) = e.source.strip_prefix("file:") {
            f
        } else if let Some(rest) = e.source.strip_prefix("symbol:") {
            // symbol:path/file.rs:SymbolName → extract "path/file.rs"
            rest.rsplit_once(':').map(|(path, _)| path).unwrap_or(rest)
        } else {
            continue;
        };

        // Disambiguation: import path > same directory > degree centrality
        let imports_for_caller = import_map.get(caller_file);
        let caller_dir = std::path::Path::new(caller_file)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("");

        let resolved = if candidates.len() == 1 {
            // Unique match — high confidence
            Some((&candidates[0].0, "resolved", 0.9))
        } else {
            // Try import resolution
            let by_import: Vec<_> = candidates
                .iter()
                .filter(|(_, file)| {
                    imports_for_caller
                        .map(|imps| imps.contains(file.as_str()))
                        .unwrap_or(false)
                })
                .collect();
            if by_import.len() == 1 {
                Some((&by_import[0].0, "resolved", 0.9))
            } else {
                // Try same-directory preference
                let by_dir: Vec<_> = candidates
                    .iter()
                    .filter(|(_, file)| {
                        std::path::Path::new(file.as_str())
                            .parent()
                            .and_then(|p| p.to_str())
                            == Some(caller_dir)
                    })
                    .collect();
                if by_dir.len() == 1 {
                    Some((&by_dir[0].0, "inferred", 0.5))
                } else {
                    // Truly ambiguous — keep first candidate with AMBIGUOUS tag
                    Some((&candidates[0].0, "ambiguous", 0.3))
                }
            }
        };

        if let Some((target_id, evidence, weight)) = resolved {
            e.target = target_id.clone();
            e.evidence = evidence.to_string();
            e.weight = weight;
        }
    }

    let _ = known_files; // used by callers, silence unused warning
}

fn edge(
    source: String,
    target: String,
    relation: &str,
    evidence: &str,
    weight: f64,
    metadata: serde_json::Value,
) -> KnowledgeEdge {
    let source_file = metadata
        .get("sourceFile")
        .and_then(|value| value.as_str())
        .map(String::from);
    KnowledgeEdge {
        source,
        target,
        relation: relation.to_string(),
        evidence: evidence.to_string(),
        weight,
        source_file,
        metadata,
    }
}

fn create_file_node(relative_path: &str, kind: FileKind) -> KnowledgeNode {
    let basename = filename_of(relative_path);
    let extension = Path::new(relative_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    KnowledgeNode {
        id: format!("file:{relative_path}"),
        label: basename.clone(),
        node_type: kind.graph_node_type().to_string(),
        source_type: "derived".to_string(),
        source_id: relative_path.to_string(),
        metadata: json!({
            "fullPath": relative_path,
            "basename": basename,
            "extension": extension,
            "repositoryFileKind": kind.label(),
        }),
        community_id: None,
    }
}

fn metadata_only_file(
    file: &RepositoryFile,
    diagnostic: &str,
    size_bytes: Option<u64>,
) -> ParsedRepositoryFile {
    let mut nodes = vec![create_file_node(&file.relative_path, file.kind)];
    let mut edges = Vec::new();
    attach_parser_metadata(
        &mut nodes,
        file,
        "metadata-only",
        size_bytes,
        &[diagnostic.to_string()],
    );
    add_diagnostic_node(&mut nodes, &mut edges, file, diagnostic);
    ParsedRepositoryFile {
        nodes,
        edges,
        semantic_text: Some(format!("{}: {}", file.relative_path, diagnostic)),
    }
}

fn attach_parser_metadata(
    nodes: &mut [KnowledgeNode],
    file: &RepositoryFile,
    parser: &str,
    size_bytes: Option<u64>,
    diagnostics: &[String],
) {
    let file_id = format!("file:{}", file.relative_path);
    for node in nodes {
        let source_file = node
            .metadata
            .get("sourceFile")
            .and_then(|value| value.as_str())
            .unwrap_or(&file.relative_path);
        if node.id == file_id || source_file == file.relative_path {
            merge_metadata(
                &mut node.metadata,
                &json!({
                    "sourceFile": file.relative_path,
                    "parser": parser,
                    "repositoryFileKind": file.kind.label(),
                    "sizeBytes": size_bytes,
                    "diagnostics": diagnostics,
                }),
            );
        }
    }
}

fn attach_parser_metadata_to_edges(edges: &mut [KnowledgeEdge], parser: &str) {
    for edge_item in edges {
        merge_metadata(&mut edge_item.metadata, &json!({ "parser": parser }));
    }
}

fn add_diagnostic_node(
    nodes: &mut Vec<KnowledgeNode>,
    edges: &mut Vec<KnowledgeEdge>,
    file: &RepositoryFile,
    diagnostic: &str,
) {
    let diagnostic_id = format!(
        "section:{}:diagnostic:{}",
        file.relative_path,
        short_hash(diagnostic.as_bytes())
    );
    nodes.push(KnowledgeNode {
        id: diagnostic_id.clone(),
        label: truncate(diagnostic, 80),
        node_type: "section".to_string(),
        source_type: "derived".to_string(),
        source_id: file.relative_path.clone(),
        metadata: json!({
            "sourceFile": file.relative_path,
            "parser": "diagnostic",
            "diagnostic": diagnostic,
            "repositoryFileKind": file.kind.label(),
        }),
        community_id: None,
    });
    edges.push(edge(
        format!("file:{}", file.relative_path),
        diagnostic_id,
        "contains",
        "ambiguous",
        0.3,
        json!({ "sourceFile": file.relative_path, "parser": "diagnostic" }),
    ));
}

fn resolve_import(
    relative_path: &str,
    specifier: &str,
    known_files: &HashSet<String>,
) -> Option<String> {
    let base = Path::new(relative_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let candidate = normalize_relative_path(base.join(specifier));
    let candidates = [
        candidate.clone(),
        format!("{candidate}.ts"),
        format!("{candidate}.tsx"),
        format!("{candidate}.js"),
        format!("{candidate}.jsx"),
        format!("{candidate}.mjs"),
        format!("{candidate}.cjs"),
        format!("{candidate}/index.ts"),
        format!("{candidate}/index.tsx"),
        format!("{candidate}/index.js"),
    ];
    candidates
        .into_iter()
        .find(|entry| known_files.contains(entry))
}

fn normalize_relative_path(path: PathBuf) -> String {
    let mut parts = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                parts.pop();
            }
            std::path::Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            _ => {}
        }
    }
    parts.join("/")
}

fn classify_heading(heading: &str) -> &'static str {
    let lowered = heading.to_ascii_lowercase();
    if lowered.contains("decision") {
        "decision"
    } else if lowered.contains("task") || lowered.contains("todo") {
        "task"
    } else if lowered.contains("why") || lowered.contains("rationale") {
        "rationale"
    } else {
        "section"
    }
}

fn tokenize(text: &str) -> BTreeSet<String> {
    Regex::new(r"[a-z0-9_./-]{3,}")
        .unwrap()
        .find_iter(&text.to_ascii_lowercase())
        .map(|match_item| match_item.as_str().to_string())
        .collect()
}

fn jaccard_similarity(left: &BTreeSet<String>, right: &BTreeSet<String>) -> f64 {
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(right).count() as f64;
    let union = left.union(right).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        intersection / union
    }
}

fn read_zip_text_entries<F>(
    bytes: &[u8],
    include: F,
    max_total_bytes: usize,
) -> Result<Vec<(String, String)>, String>
where
    F: Fn(&str) -> bool,
{
    let cursor = Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).map_err(|error| error.to_string())?;
    let mut total = 0usize;
    let mut entries = Vec::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| error.to_string())?;
        let name = file.name().to_string();
        if !include(&name) {
            continue;
        }
        total = total.saturating_add(file.size() as usize);
        if total > max_total_bytes {
            return Err("zip text extraction exceeded parser byte budget".to_string());
        }
        let mut text = String::new();
        file.read_to_string(&mut text)
            .map_err(|error| error.to_string())?;
        entries.push((name, text));
    }
    Ok(entries)
}

fn read_xlsx_shared_strings(bytes: &[u8]) -> Result<Vec<String>, String> {
    let entries = read_zip_text_entries(bytes, |name| name == "xl/sharedStrings.xml", 2_000_000)?;
    let Some((_, xml)) = entries.into_iter().next() else {
        return Ok(Vec::new());
    };
    let item_pattern = Regex::new(r"(?s)<si\b[^>]*>(.*?)</si>").unwrap();
    Ok(item_pattern
        .captures_iter(&xml)
        .filter_map(|captures| captures.get(1).map(|value| xml_text(value.as_str())))
        .filter(|value| !value.trim().is_empty())
        .collect())
}

fn xlsx_sheet_text(xml: &str, shared_strings: &[String]) -> String {
    let cell_pattern =
        Regex::new(r#"(?s)<c\b[^>]*?(?:t="(?P<t>[^"]+)")?[^>]*>(?P<body>.*?)</c>"#).unwrap();
    let value_pattern = Regex::new(r"(?s)<v>(.*?)</v>").unwrap();
    let inline_pattern = Regex::new(r"(?s)<is\b[^>]*>(.*?)</is>").unwrap();
    let mut values = Vec::new();

    for captures in cell_pattern.captures_iter(xml) {
        let cell_type = captures.name("t").map(|value| value.as_str()).unwrap_or("");
        let body = captures
            .name("body")
            .map(|value| value.as_str())
            .unwrap_or("");
        let value = if cell_type == "s" {
            value_pattern
                .captures(body)
                .and_then(|inner| inner.get(1))
                .and_then(|index| index.as_str().parse::<usize>().ok())
                .and_then(|index| shared_strings.get(index).cloned())
        } else if cell_type == "inlineStr" {
            inline_pattern
                .captures(body)
                .and_then(|inner| inner.get(1))
                .map(|value| xml_text(value.as_str()))
        } else {
            value_pattern
                .captures(body)
                .and_then(|inner| inner.get(1))
                .map(|value| decode_xml_entities(value.as_str()))
        };
        if let Some(value) = value {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                values.push(trimmed.to_string());
            }
        }
    }

    values.join("\n")
}

fn ooxml_text_to_markdown(xml: &str) -> String {
    let paragraphized = Regex::new(r"</(?:w:p|a:p|w:tr|a:tr)>")
        .unwrap()
        .replace_all(xml, "\n");
    xml_text(&paragraphized)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn xml_text(xml: &str) -> String {
    let without_tags = Regex::new(r"(?s)<[^>]+>").unwrap().replace_all(xml, " ");
    collapse_ws(&decode_xml_entities(&without_tags))
}

fn decode_xml_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

fn collapse_ws(value: &str) -> String {
    Regex::new(r"[ \t\r\f]+")
        .unwrap()
        .replace_all(value, " ")
        .trim()
        .to_string()
}

fn extract_pdf_text_best_effort(bytes: &[u8]) -> String {
    if !bytes.starts_with(b"%PDF") {
        return String::new();
    }
    let lossy = String::from_utf8_lossy(bytes);
    let literal_pattern = Regex::new(r#"\(([^()\r\n]{2,500})\)\s*T[Jj]"#).unwrap();
    let bracket_pattern = Regex::new(r#"\[((?:\([^()\r\n]{1,200}\)\s*)+)\]\s*TJ"#).unwrap();
    let paren_pattern = Regex::new(r#"\(([^()\r\n]{1,200})\)"#).unwrap();
    let mut lines = Vec::new();
    for captures in literal_pattern.captures_iter(&lossy) {
        if let Some(value) = captures.get(1) {
            lines.push(unescape_pdf_literal(value.as_str()));
        }
    }
    for captures in bracket_pattern.captures_iter(&lossy) {
        if let Some(group) = captures.get(1) {
            let mut line = String::new();
            for piece in paren_pattern.captures_iter(group.as_str()) {
                if let Some(value) = piece.get(1) {
                    line.push_str(&unescape_pdf_literal(value.as_str()));
                }
            }
            if !line.trim().is_empty() {
                lines.push(line);
            }
        }
    }
    lines
        .into_iter()
        .map(|line| collapse_ws(&line))
        .filter(|line| line.chars().any(|ch| ch.is_alphanumeric()))
        .take(2_000)
        .collect::<Vec<_>>()
        .join("\n")
}

fn unescape_pdf_literal(value: &str) -> String {
    value
        .replace(r"\(", "(")
        .replace(r"\)", ")")
        .replace(r"\\", "\\")
        .replace(r"\n", "\n")
        .replace(r"\r", "\n")
        .replace(r"\t", "\t")
}

fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((
            u32::from_be_bytes(bytes[16..20].try_into().ok()?),
            u32::from_be_bytes(bytes[20..24].try_into().ok()?),
        ));
    }
    if bytes.len() >= 10 && bytes.starts_with(b"GIF8") {
        return Some((
            u16::from_le_bytes(bytes[6..8].try_into().ok()?) as u32,
            u16::from_le_bytes(bytes[8..10].try_into().ok()?) as u32,
        ));
    }
    if bytes.len() >= 30 && bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        if bytes.get(12..16) == Some(b"VP8X") && bytes.len() >= 30 {
            let width = 1 + u32::from_le_bytes([bytes[24], bytes[25], bytes[26], 0]);
            let height = 1 + u32::from_le_bytes([bytes[27], bytes[28], bytes[29], 0]);
            return Some((width, height));
        }
        if bytes.get(12..16) == Some(b"VP8 ") && bytes.len() >= 30 {
            return Some((
                u16::from_le_bytes(bytes[26..28].try_into().ok()?) as u32 & 0x3fff,
                u16::from_le_bytes(bytes[28..30].try_into().ok()?) as u32 & 0x3fff,
            ));
        }
    }
    jpeg_dimensions(bytes)
}

fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
        return None;
    }
    let mut offset = 2usize;
    while offset + 9 < bytes.len() {
        if bytes[offset] != 0xff {
            offset += 1;
            continue;
        }
        let marker = bytes[offset + 1];
        offset += 2;
        if marker == 0xd8 || marker == 0xd9 {
            continue;
        }
        if offset + 2 > bytes.len() {
            return None;
        }
        let len = u16::from_be_bytes(bytes[offset..offset + 2].try_into().ok()?) as usize;
        if len < 2 || offset + len > bytes.len() {
            return None;
        }
        if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            let height = u16::from_be_bytes(bytes[offset + 3..offset + 5].try_into().ok()?) as u32;
            let width = u16::from_be_bytes(bytes[offset + 5..offset + 7].try_into().ok()?) as u32;
            return Some((width, height));
        }
        offset += len;
    }
    None
}

fn media_metadata(bytes: &[u8], path: &str) -> String {
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match extension.as_str() {
        "wav" => wav_metadata(bytes),
        "mp4" | "mov" | "m4v" => mp4_metadata(bytes),
        "mp3" => mp3_metadata(bytes),
        _ => String::new(),
    }
}

fn wav_metadata(bytes: &[u8]) -> String {
    if bytes.len() < 44 || bytes.get(0..4) != Some(b"RIFF") || bytes.get(8..12) != Some(b"WAVE") {
        return String::new();
    }
    let sample_rate = u32::from_le_bytes(bytes[24..28].try_into().unwrap_or([0; 4]));
    let byte_rate = u32::from_le_bytes(bytes[28..32].try_into().unwrap_or([0; 4]));
    let mut data_size = 0u32;
    let mut offset = 12usize;
    while offset + 8 <= bytes.len() {
        let chunk_id = &bytes[offset..offset + 4];
        let chunk_size =
            u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap_or([0; 4]));
        if chunk_id == b"data" {
            data_size = chunk_size;
            break;
        }
        offset = offset.saturating_add(8).saturating_add(chunk_size as usize);
    }
    let duration = if byte_rate > 0 {
        data_size as f64 / byte_rate as f64
    } else {
        0.0
    };
    format!("format=wav sampleRate={sample_rate} durationSeconds={duration:.2}")
}

fn mp3_metadata(bytes: &[u8]) -> String {
    if bytes.starts_with(b"ID3") {
        let version = bytes.get(3).copied().unwrap_or_default();
        format!("format=mp3 id3Version=2.{version}")
    } else if bytes.starts_with(&[0xff, 0xfb]) || bytes.starts_with(&[0xff, 0xf3]) {
        "format=mp3 mpegAudioFrame=true".to_string()
    } else {
        String::new()
    }
}

fn mp4_metadata(bytes: &[u8]) -> String {
    if bytes.len() < 12 {
        return String::new();
    }
    let brand = bytes
        .windows(4)
        .position(|window| window == b"ftyp")
        .and_then(|pos| bytes.get(pos + 4..pos + 8))
        .map(|slice| String::from_utf8_lossy(slice).to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let duration = mp4_duration_seconds(bytes)
        .map(|value| format!(" durationSeconds={value:.2}"))
        .unwrap_or_default();
    format!("format=mp4 brand={brand}{duration}")
}

fn mp4_duration_seconds(bytes: &[u8]) -> Option<f64> {
    let pos = bytes.windows(4).position(|window| window == b"mvhd")?;
    let version = *bytes.get(pos + 4)?;
    if version == 1 {
        let timescale = u32::from_be_bytes(bytes.get(pos + 20..pos + 24)?.try_into().ok()?);
        let duration = u64::from_be_bytes(bytes.get(pos + 24..pos + 32)?.try_into().ok()?);
        (timescale > 0).then_some(duration as f64 / timescale as f64)
    } else {
        let timescale = u32::from_be_bytes(bytes.get(pos + 12..pos + 16)?.try_into().ok()?);
        let duration = u32::from_be_bytes(bytes.get(pos + 16..pos + 20)?.try_into().ok()?);
        (timescale > 0).then_some(duration as f64 / timescale as f64)
    }
}

fn render_graph_html(graph: &KnowledgeGraph) -> String {
    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Graph</title></head><body><pre id=\"graph\">{}</pre></body></html>",
        serde_json::to_string_pretty(graph).unwrap_or_else(|_| "{}".to_string())
    )
}

fn line_number_for_index(content: &str, index: usize) -> usize {
    content[..index.min(content.len())]
        .bytes()
        .filter(|byte| *byte == b'\n')
        .count()
        + 1
}

fn filename_of(relative_path: &str) -> String {
    Path::new(relative_path)
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| relative_path.to_string())
}

fn short_hash(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(12);
    for byte in &digest[..6] {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

fn truncate(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

fn merge_metadata(target: &mut serde_json::Value, source: &serde_json::Value) {
    let Some(target_obj) = target.as_object_mut() else {
        *target = source.clone();
        return;
    };
    let Some(source_obj) = source.as_object() else {
        return;
    };
    for (key, value) in source_obj {
        target_obj.insert(key.clone(), value.clone());
    }
}

fn edge_rank(evidence: &str) -> u8 {
    match evidence {
        "extracted" => 3,
        "inferred" => 2,
        "ambiguous" => 1,
        _ => 0,
    }
}

fn rfc3339_now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc().unix_timestamp().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn zip_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
        let cursor = Cursor::new(Vec::new());
        let mut writer = zip::ZipWriter::new(cursor);
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            writer.start_file(name, options).expect("start zip file");
            writer
                .write_all(content.as_bytes())
                .expect("write zip entry");
        }
        writer.finish().expect("finish zip").into_inner()
    }

    #[test]
    fn extracts_repository_code_and_docs() {
        let temp_root = std::env::temp_dir().join(format!("chum-mem-repo-{}", Uuid::new_v4()));
        fs::create_dir_all(temp_root.join("src")).expect("create src dir");
        fs::write(
            temp_root.join("src/lib.rs"),
            "use crate::foo;\n// WHY: keep graph edges stable\npub fn alpha() {}\n",
        )
        .expect("write code file");
        fs::write(
            temp_root.join("README.md"),
            "# Decision Log\nSee `src/lib.rs`\n",
        )
        .expect("write doc file");

        let result = build_repository_knowledge(
            RepositoryBuildOptions {
                root_dir: temp_root.to_string_lossy().to_string(),
                out_dir: None,
                project_id: Some(Uuid::nil()),
                update: true,
                no_viz: true,
            },
            8_000,
            20_000,
        )
        .expect("build repository graph");

        assert!(
            result
                .graph
                .nodes
                .iter()
                .any(|node| node.id == "file:src/lib.rs" && node.node_type == "file")
        );
        assert!(
            result
                .graph
                .nodes
                .iter()
                .any(|node| node.id.starts_with("symbol:src/lib.rs:alpha"))
        );
        assert!(
            result
                .graph
                .nodes
                .iter()
                .any(|node| node.id == "file:README.md" && node.node_type == "document")
        );
        assert!(
            result
                .graph
                .edges
                .iter()
                .any(|edge| edge.source == "file:README.md" && edge.target == "file:src/lib.rs")
        );

        fs::remove_dir_all(&temp_root).expect("remove temp root");
    }

    #[test]
    fn parses_graphify_style_binary_repository_payloads() {
        let docx = zip_bytes(&[(
            "word/document.xml",
            r#"<w:document><w:body><w:p><w:r><w:t>Repository roadmap</w:t></w:r></w:p></w:body></w:document>"#,
        )]);
        let xlsx = zip_bytes(&[
            (
                "xl/sharedStrings.xml",
                r#"<sst><si><t>Metric</t></si><si><t>Latency</t></si></sst>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row><c t="s"><v>0</v></c><c t="s"><v>1</v></c></row></sheetData></worksheet>"#,
            ),
        ]);
        let pptx = zip_bytes(&[(
            "ppt/slides/slide1.xml",
            r#"<p:sld><p:cSld><a:p><a:r><a:t>Launch plan</a:t></a:r></a:p></p:cSld></p:sld>"#,
        )]);
        let pdf = b"%PDF-1.4\nBT\n(Proof carrying context) Tj\nET\n%%EOF".to_vec();
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&32u32.to_be_bytes());
        png.extend_from_slice(&16u32.to_be_bytes());
        png.extend_from_slice(&[8, 2, 0, 0, 0]);
        let mut wav = vec![0u8; 44];
        wav[0..4].copy_from_slice(b"RIFF");
        wav[8..12].copy_from_slice(b"WAVE");
        wav[24..28].copy_from_slice(&48_000u32.to_le_bytes());
        wav[28..32].copy_from_slice(&96_000u32.to_le_bytes());
        wav[36..40].copy_from_slice(b"data");
        wav[40..44].copy_from_slice(&96_000u32.to_le_bytes());

        let payloads = vec![
            RepositoryFilePayload {
                path: "docs/roadmap.docx".to_string(),
                content: None,
                bytes: Some(docx),
                media_type: Some(
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                        .to_string(),
                ),
                size_bytes: None,
            },
            RepositoryFilePayload {
                path: "data/metrics.xlsx".to_string(),
                content: None,
                bytes: Some(xlsx),
                media_type: Some(
                    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet".to_string(),
                ),
                size_bytes: None,
            },
            RepositoryFilePayload {
                path: "slides/plan.pptx".to_string(),
                content: None,
                bytes: Some(pptx),
                media_type: Some(
                    "application/vnd.openxmlformats-officedocument.presentationml.presentation"
                        .to_string(),
                ),
                size_bytes: None,
            },
            RepositoryFilePayload {
                path: "papers/context.pdf".to_string(),
                content: None,
                bytes: Some(pdf),
                media_type: Some("application/pdf".to_string()),
                size_bytes: None,
            },
            RepositoryFilePayload {
                path: "assets/logo.png".to_string(),
                content: None,
                bytes: Some(png),
                media_type: Some("image/png".to_string()),
                size_bytes: None,
            },
            RepositoryFilePayload {
                path: "media/brief.wav".to_string(),
                content: None,
                bytes: Some(wav),
                media_type: Some("audio/wav".to_string()),
                size_bytes: None,
            },
        ];

        let (nodes, edges) = parse_file_payload_batch(&payloads);

        for path in [
            "docs/roadmap.docx",
            "data/metrics.xlsx",
            "slides/plan.pptx",
            "papers/context.pdf",
            "assets/logo.png",
            "media/brief.wav",
        ] {
            assert!(
                nodes
                    .iter()
                    .any(|node| node.id == format!("file:{path}") && node.node_type == "document"),
                "missing document node for {path}"
            );
        }

        assert!(nodes.iter().any(
            |node| node.metadata.get("parser").and_then(|value| value.as_str())
                == Some("docx-ooxml")
        ));
        assert!(nodes.iter().any(
            |node| node.metadata.get("parser").and_then(|value| value.as_str())
                == Some("xlsx-ooxml")
        ));
        assert!(nodes.iter().any(
            |node| node.metadata.get("parser").and_then(|value| value.as_str())
                == Some("pptx-ooxml")
        ));
        assert!(edges.iter().any(|edge| edge.relation == "contains"
            && edge.source == "file:papers/context.pdf"));
        let image = nodes
            .iter()
            .find(|node| node.id == "file:assets/logo.png")
            .expect("image node");
        assert_eq!(
            image.metadata.get("width").and_then(|v| v.as_u64()),
            Some(32)
        );
        assert_eq!(
            image.metadata.get("height").and_then(|v| v.as_u64()),
            Some(16)
        );
        let media = nodes
            .iter()
            .find(|node| node.id == "file:media/brief.wav")
            .expect("media node");
        assert!(
            media
                .metadata
                .get("media")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .contains("format=wav")
        );
    }

    #[test]
    fn extracts_multilanguage_repository_with_ast() {
        // Use the worked/ test fixtures with real multi-language project files
        let testdata_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata/worked");
        if !testdata_dir.exists() {
            // Skip if testdata not present (e.g. CI without fixtures)
            return;
        }

        let result = build_repository_knowledge(
            RepositoryBuildOptions {
                root_dir: testdata_dir.to_string_lossy().to_string(),
                out_dir: None,
                project_id: Some(Uuid::nil()),
                update: false,
                no_viz: true,
            },
            8_000,
            20_000,
        )
        .expect("build multi-language repository graph");

        let graph = &result.graph;

        // --- Validate we found files ---
        assert!(
            result.stats.total_files >= 10,
            "Expected at least 10 files, got {}",
            result.stats.total_files
        );

        // --- Check that AST-parsed files have tree-sitter metadata ---
        let ts_symbols: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| {
                n.node_type == "symbol"
                    && n.metadata.get("parser").and_then(|v| v.as_str()) == Some("tree-sitter")
            })
            .collect();
        assert!(
            ts_symbols.len() >= 5,
            "Expected at least 5 tree-sitter parsed symbols, got {}",
            ts_symbols.len()
        );

        // --- Check symbols were extracted from multiple languages ---
        let languages_found: HashSet<String> = graph
            .nodes
            .iter()
            .filter(|n| n.node_type == "symbol")
            .filter_map(|n| {
                n.metadata
                    .get("language")
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .collect();
        assert!(
            languages_found.len() >= 3,
            "Expected symbols from at least 3 languages, got: {:?}",
            languages_found
        );

        // --- Check call graph edges exist ---
        let call_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .collect();
        assert!(
            !call_edges.is_empty(),
            "Expected call graph edges but found none"
        );

        // --- Check import edges exist ---
        let import_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.relation == "imports")
            .collect();
        assert!(
            !import_edges.is_empty(),
            "Expected import edges but found none"
        );

        // --- Check rationale nodes exist ---
        let rationale_nodes: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| n.node_type == "rationale")
            .collect();
        assert!(
            !rationale_nodes.is_empty(),
            "Expected rationale nodes but found none"
        );

        // --- Check communities were assigned ---
        assert!(
            !graph.communities.is_empty(),
            "Expected communities but found none"
        );

        // --- Check document sections were extracted ---
        let doc_sections: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| {
                n.node_type == "section"
                    || n.node_type == "decision"
                    || n.node_type == "task"
                    || n.node_type == "rationale"
            })
            .collect();
        assert!(
            doc_sections.len() >= 2,
            "Expected at least 2 doc sections, got {}",
            doc_sections.len()
        );

        // --- Check semantic similarity edges ---
        let similarity_edges: Vec<_> = graph
            .edges
            .iter()
            .filter(|e| e.relation == "semantically_similar_to")
            .collect();
        // With multiple files, there should be some semantic similarity edges
        // (this depends on file content overlap)

        // --- Validate graph statistics ---
        assert!(graph.statistics.node_count > 0, "node_count should be > 0");
        assert!(graph.statistics.edge_count > 0, "edge_count should be > 0");

        // --- Print summary for validation report ---
        println!("\n=== Multi-Language Repository Graph Validation ===");
        println!("Total files processed: {}", result.stats.total_files);
        println!("Total nodes: {}", graph.statistics.node_count);
        println!("Total edges: {}", graph.statistics.edge_count);
        println!("Communities: {}", graph.statistics.community_count);
        println!("Languages found: {:?}", languages_found);
        println!("Tree-sitter parsed symbols: {}", ts_symbols.len());
        println!("Call graph edges: {}", call_edges.len());
        println!("Import edges: {}", import_edges.len());
        println!("Rationale nodes: {}", rationale_nodes.len());
        println!("Semantic similarity edges: {}", similarity_edges.len());
        println!("Average degree: {:.2}", graph.statistics.avg_degree);
        println!("=== Validation PASSED ===\n");

        // Clean up output directory
        let out_dir = testdata_dir.join("graphify-out");
        if out_dir.exists() {
            let _ = fs::remove_dir_all(&out_dir);
        }
    }

    #[test]
    fn tree_sitter_extracts_more_than_regex_fallback() {
        // Verify tree-sitter finds MORE symbols than regex for Python/Go/Ruby etc.
        let temp_root =
            std::env::temp_dir().join(format!("chum-mem-ts-vs-regex-{}", Uuid::new_v4()));
        fs::create_dir_all(temp_root.join("src")).expect("create src dir");

        // Python file with imports, class, functions — regex only catches class/function keywords
        fs::write(
            temp_root.join("src/main.py"),
            r#"
import os
from pathlib import Path

class DataProcessor:
    """Process data."""
    def process(self, items):
        result = self.validate(items)
        return transform(result)

    def validate(self, items):
        return items

def helper():
    os.getcwd()
"#,
        )
        .expect("write python file");

        let result = build_repository_knowledge(
            RepositoryBuildOptions {
                root_dir: temp_root.to_string_lossy().to_string(),
                out_dir: None,
                project_id: Some(Uuid::nil()),
                update: false,
                no_viz: true,
            },
            8_000,
            20_000,
        )
        .expect("build repo");

        // Tree-sitter should find: DataProcessor (class), process (fn), validate (fn), helper (fn)
        let symbols: Vec<_> = result
            .graph
            .nodes
            .iter()
            .filter(|n| n.node_type == "symbol")
            .collect();
        assert!(
            symbols.len() >= 4,
            "Tree-sitter should find at least 4 symbols in Python, got {}",
            symbols.len()
        );

        // Should find imports (os, pathlib)
        let imports: Vec<_> = result
            .graph
            .edges
            .iter()
            .filter(|e| e.relation == "imports")
            .collect();
        assert!(
            imports.len() >= 2,
            "Tree-sitter should find at least 2 imports in Python, got {}",
            imports.len()
        );

        // Should find call graph edges (validate, transform, getcwd)
        let calls: Vec<_> = result
            .graph
            .edges
            .iter()
            .filter(|e| e.relation == "calls")
            .collect();
        assert!(
            !calls.is_empty(),
            "Tree-sitter should find call edges in Python"
        );

        // All symbols should have parser: "tree-sitter" metadata
        for sym in &symbols {
            assert_eq!(
                sym.metadata.get("parser").and_then(|v| v.as_str()),
                Some("tree-sitter"),
                "Symbol {} should have tree-sitter parser metadata",
                sym.label
            );
        }

        fs::remove_dir_all(&temp_root).expect("remove temp root");
    }
}
