use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
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

/// Skip files larger than 256 KB — they're usually generated, vendored, or data dumps.
const MAX_FILE_SIZE_BYTES: u64 = 256 * 1024;

const CODE_EXTENSIONS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "rs", "java", "c", "cc", "cpp", "h", "hpp",
    "rb", "cs", "kt", "scala", "php", "swift", "lua", "zig", "ps1", "sh", "sql", "html", "htm",
    "css", "scss", "sass", "less", "vue", "svelte", "astro",
];
const DOC_EXTENSIONS: &[&str] = &["md", "mdx", "txt", "rst", "yaml", "yml", "json", "jsonc"];
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
}

#[derive(Debug, Clone)]
struct RepositoryFile {
    absolute_path: PathBuf,
    relative_path: String,
    kind: FileKind,
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
        if meta.len() > MAX_FILE_SIZE_BYTES {
            skipped_files += 1;
            continue;
        }

        // Read as bytes first, then convert to UTF-8 — skip non-text files gracefully
        let bytes = match fs::read(&file.absolute_path) {
            Ok(b) => b,
            Err(_) => {
                skipped_files += 1;
                continue;
            }
        };
        let text = match String::from_utf8(bytes) {
            Ok(s) => s,
            Err(_) => {
                skipped_files += 1;
                continue;
            }
        };

        match file.kind {
            FileKind::Code => {
                let extracted = extract_code_file(file, &text, &known_files);
                nodes.extend(extracted.0);
                edges.extend(extracted.1);
            }
            FileKind::Doc => {
                let extracted = extract_document_file(file, &text, &known_files);
                nodes.extend(extracted.0);
                edges.extend(extracted.1);
            }
        }
        semantic_inputs.push((file.relative_path.clone(), text));
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
    pub ignore_dirs: Vec<String>,
    pub ignore_files: Vec<String>,
    pub ignore_patterns: Vec<String>,
    pub max_file_size_bytes: u64,
}

pub fn sync_rules() -> SyncRules {
    SyncRules {
        code_extensions: CODE_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
        doc_extensions: DOC_EXTENSIONS.iter().map(|s| s.to_string()).collect(),
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
    }
}

pub fn parse_file_batch(files: &[(String, String)]) -> (Vec<KnowledgeNode>, Vec<KnowledgeEdge>) {
    let known_files: HashSet<String> = files.iter().map(|(path, _)| path.clone()).collect();

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut semantic_inputs = Vec::new();

    for (path, content) in files {
        if content.len() as u64 > MAX_FILE_SIZE_BYTES {
            continue;
        }

        let Some(kind) = classify_file_kind(Path::new(path)) else {
            continue;
        };

        let repo_file = RepositoryFile {
            absolute_path: PathBuf::from(path),
            relative_path: path.clone(),
            kind,
        };

        match kind {
            FileKind::Code => {
                let extracted = extract_code_file(&repo_file, content, &known_files);
                nodes.extend(extracted.0);
                edges.extend(extracted.1);
            }
            FileKind::Doc => {
                let extracted = extract_document_file(&repo_file, content, &known_files);
                nodes.extend(extracted.0);
                edges.extend(extracted.1);
            }
        }
        semantic_inputs.push((path.clone(), content.clone()));
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
    KnowledgeEdge {
        source,
        target,
        relation: relation.to_string(),
        evidence: evidence.to_string(),
        weight,
        source_file: None,
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
        node_type: if kind == FileKind::Doc {
            "document".to_string()
        } else {
            "file".to_string()
        },
        source_type: "derived".to_string(),
        source_id: relative_path.to_string(),
        metadata: json!({
            "fullPath": relative_path,
            "basename": basename,
            "extension": extension,
        }),
        community_id: None,
    }
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
