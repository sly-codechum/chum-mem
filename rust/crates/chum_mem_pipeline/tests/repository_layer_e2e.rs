use std::collections::HashSet;
use std::io::{Cursor, Write};

use chum_mem_pipeline::{
    EvidenceDistribution, GraphStatistics, KnowledgeGraph, RepositoryFilePayload,
    parse_file_payload_batch, run_knowledge_query, sync_rules,
};
use uuid::Uuid;

const REQUIRED_CODE_EXTENSIONS: &[&str] = &[
    "py", "ts", "js", "jsx", "tsx", "go", "rs", "java", "c", "cpp", "rb", "cs", "kt", "scala",
    "php", "swift", "lua", "zig", "ps1", "ex", "exs", "m", "jl", "vue", "svelte", "sql",
];

const REQUIRED_DOC_EXTENSIONS: &[&str] = &["md", "mdx", "html", "txt", "rst", "yaml", "yml"];
const REQUIRED_BINARY_EXTENSIONS: &[&str] = &[
    "docx", "xlsx", "pptx", "pdf", "png", "jpg", "webp", "gif", "mp4", "mov", "mp3", "wav",
];

fn text_payload(path: impl Into<String>, content: impl Into<String>) -> RepositoryFilePayload {
    let content = content.into();
    RepositoryFilePayload {
        path: path.into(),
        size_bytes: Some(content.len() as u64),
        content: Some(content),
        bytes: None,
        media_type: None,
    }
}

fn binary_payload(
    path: impl Into<String>,
    bytes: Vec<u8>,
    media_type: impl Into<String>,
) -> RepositoryFilePayload {
    RepositoryFilePayload {
        path: path.into(),
        size_bytes: Some(bytes.len() as u64),
        content: None,
        bytes: Some(bytes),
        media_type: Some(media_type.into()),
    }
}

fn zip_bytes(entries: &[(&str, &str)]) -> Vec<u8> {
    let cursor = Cursor::new(Vec::new());
    let mut writer = zip::ZipWriter::new(cursor);
    let options = zip::write::SimpleFileOptions::default();
    for (name, content) in entries {
        writer.start_file(name, options).expect("start zip entry");
        writer
            .write_all(content.as_bytes())
            .expect("write zip entry");
    }
    writer.finish().expect("finish zip").into_inner()
}

fn docx_fixture() -> Vec<u8> {
    zip_bytes(&[(
        "word/document.xml",
        r#"<w:document><w:body><w:p><w:r><w:t>Repository QA roadmap</w:t></w:r></w:p></w:body></w:document>"#,
    )])
}

fn xlsx_fixture() -> Vec<u8> {
    zip_bytes(&[
        (
            "xl/sharedStrings.xml",
            r#"<sst><si><t>Metric</t></si><si><t>Latency</t></si><si><t>42</t></si></sst>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet><sheetData><row><c t="s"><v>0</v></c><c t="s"><v>1</v></c><c t="s"><v>2</v></c></row></sheetData></worksheet>"#,
        ),
    ])
}

fn pptx_fixture() -> Vec<u8> {
    zip_bytes(&[(
        "ppt/slides/slide1.xml",
        r#"<p:sld><p:cSld><a:p><a:r><a:t>AI client launch plan</a:t></a:r></a:p></p:cSld></p:sld>"#,
    )])
}

fn pdf_fixture() -> Vec<u8> {
    b"%PDF-1.4\nBT\n(Proof carrying context) Tj\nET\n%%EOF".to_vec()
}

fn png_fixture() -> Vec<u8> {
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend_from_slice(&32u32.to_be_bytes());
    png.extend_from_slice(&16u32.to_be_bytes());
    png.extend_from_slice(&[8, 2, 0, 0, 0]);
    png
}

fn jpg_fixture() -> Vec<u8> {
    vec![
        0xff, 0xd8, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x10, 0x00, 0x20, 0x03, 0x01, 0x11, 0x00,
        0x02, 0x11, 0x00, 0x03, 0x11, 0x00, 0xff, 0xd9,
    ]
}

fn webp_fixture() -> Vec<u8> {
    let mut webp = vec![0u8; 30];
    webp[0..4].copy_from_slice(b"RIFF");
    webp[8..12].copy_from_slice(b"WEBP");
    webp[12..16].copy_from_slice(b"VP8X");
    webp[24] = 31;
    webp[27] = 15;
    webp
}

fn gif_fixture() -> Vec<u8> {
    let mut gif = b"GIF89a".to_vec();
    gif.extend_from_slice(&32u16.to_le_bytes());
    gif.extend_from_slice(&16u16.to_le_bytes());
    gif.extend_from_slice(&[0x80, 0, 0, 0, 0, 0]);
    gif
}

fn wav_fixture() -> Vec<u8> {
    let mut wav = vec![0u8; 44];
    wav[0..4].copy_from_slice(b"RIFF");
    wav[8..12].copy_from_slice(b"WAVE");
    wav[24..28].copy_from_slice(&48_000u32.to_le_bytes());
    wav[28..32].copy_from_slice(&96_000u32.to_le_bytes());
    wav[36..40].copy_from_slice(b"data");
    wav[40..44].copy_from_slice(&96_000u32.to_le_bytes());
    wav
}

fn mp3_fixture() -> Vec<u8> {
    b"ID3\x04\0\0\0\0\0\x10repository audio".to_vec()
}

fn mp4_fixture(brand: &[u8; 4]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&24u32.to_be_bytes());
    bytes.extend_from_slice(b"ftyp");
    bytes.extend_from_slice(brand);
    bytes.extend_from_slice(&[0; 12]);
    bytes
}

fn required_code_payload(extension: &str) -> RepositoryFilePayload {
    let path = format!("src/sample.{extension}");
    let content = match extension {
        "py" => "import os\n# WHY: preserve comments\ndef alpha_py():\n    return 'ok'\n",
        "ts" => {
            "import { helper } from './helper';\n/** docs */\nexport function alphaTs(): string { return helper(); }\n"
        }
        "js" => "function alphaJs() { return 'ok'; }\nmodule.exports = { alphaJs };\n",
        "jsx" => "export function AlphaJsx() { return <div>ok</div>; }\n",
        "tsx" => "export function AlphaTsx(): JSX.Element { return <div>ok</div>; }\n",
        "go" => "package main\nfunc AlphaGo() string { return \"ok\" }\n",
        "rs" => "pub fn alpha_rs() -> &'static str { \"ok\" }\n",
        "java" => "class AlphaJava { void run() {} }\n",
        "c" => "int alpha_c(void) { return 1; }\n",
        "cpp" => "class AlphaCpp { public: int run(){ return 1; } };\n",
        "rb" => "class AlphaRb\n  def run\n  end\nend\n",
        "cs" => "class AlphaCs { string Run() { return \"ok\"; } }\n",
        "kt" => "class AlphaKt { fun run(): String = \"ok\" }\n",
        "scala" => "class AlphaScala { def run(): String = \"ok\" }\n",
        "php" => "<?php class AlphaPhp { function run() { return 'ok'; } }\n",
        "swift" => "class AlphaSwift { func run() -> String { return \"ok\" } }\n",
        "lua" => "function alpha_lua() return 'ok' end\n",
        "zig" => "pub fn alphaZig() void {}\n",
        "ps1" => "function Invoke-Alpha { return \"ok\" }\n",
        "ex" => "defmodule AlphaEx do\n  def run, do: :ok\nend\n",
        "exs" => "defmodule AlphaExs do\n  def run, do: :ok\nend\n",
        "m" => "function y = alpha_m(x)\ny = x;\nend\n",
        "jl" => "function alpha_jl(x)\n    x\nend\n",
        "vue" => {
            "<script setup lang=\"ts\">\nfunction alphaVue(){return 1}\n</script><template><div /></template>\n"
        }
        "svelte" => "<script>function alphaSvelte(){return 1}</script><h1>ok</h1>\n",
        "sql" => "CREATE TABLE alpha_sql (id integer);\n-- WHY: preserve DDL rationale\n",
        other => panic!("missing fixture for {other}"),
    };
    text_payload(path, content)
}

fn required_doc_payload(extension: &str) -> RepositoryFilePayload {
    let path = format!("docs/sample.{extension}");
    let content = match extension {
        "md" => {
            "# Repository QA Guide\nSee [architecture](./architecture.md).\n\n```rust\nfn example() {}\n```\n"
        }
        "mdx" => "---\ntitle: QA\n---\n\n# MDX Guide\n<MetricCard value={42} />\n",
        "html" => {
            "<!doctype html><h1>HTML Guide</h1><script>alert('x')</script><p>meaningful text</p>"
        }
        "txt" => "Plain text repository note\nwith unicode: café\n",
        "rst" => "Repository Guide\n================\n\n.. note:: handled gracefully\n",
        "yaml" => "service:\n  name: chum-mem\n  nested:\n    enabled: true\n",
        "yml" => "repository:\n  layer: true\n",
        other => panic!("missing fixture for {other}"),
    };
    text_payload(path, content)
}

fn required_binary_payload(extension: &str) -> RepositoryFilePayload {
    match extension {
        "docx" => binary_payload(
            "docs/sample.docx",
            docx_fixture(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        "xlsx" => binary_payload(
            "data/sample.xlsx",
            xlsx_fixture(),
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
        "pptx" => binary_payload(
            "slides/sample.pptx",
            pptx_fixture(),
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        ),
        "pdf" => binary_payload("docs/sample.pdf", pdf_fixture(), "application/pdf"),
        "png" => binary_payload("assets/sample.png", png_fixture(), "image/png"),
        "jpg" => binary_payload("assets/sample.jpg", jpg_fixture(), "image/jpeg"),
        "webp" => binary_payload("assets/sample.webp", webp_fixture(), "image/webp"),
        "gif" => binary_payload("assets/sample.gif", gif_fixture(), "image/gif"),
        "mp4" => binary_payload("media/sample.mp4", mp4_fixture(b"isom"), "video/mp4"),
        "mov" => binary_payload("media/sample.mov", mp4_fixture(b"qt  "), "video/quicktime"),
        "mp3" => binary_payload("media/sample.mp3", mp3_fixture(), "audio/mpeg"),
        "wav" => binary_payload("media/sample.wav", wav_fixture(), "audio/wav"),
        other => panic!("missing fixture for {other}"),
    }
}

#[test]
fn sync_rules_advertise_required_multiformat_contract() {
    let rules = sync_rules();
    let code = rules
        .code_extensions
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let docs = rules
        .doc_extensions
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let binary = rules
        .binary_extensions
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();

    for extension in REQUIRED_CODE_EXTENSIONS {
        assert!(
            code.contains(extension),
            "missing code extension {extension}"
        );
    }
    for extension in REQUIRED_DOC_EXTENSIONS {
        assert!(
            docs.contains(extension),
            "missing document extension {extension}"
        );
    }
    for extension in REQUIRED_BINARY_EXTENSIONS {
        assert!(
            docs.contains(extension),
            "binary extension {extension} must be included in docExtensions for old clients"
        );
        assert!(
            binary.contains(extension),
            "missing binary extension {extension}"
        );
    }
    assert_eq!(rules.max_file_size_bytes, 256 * 1024);
    assert_eq!(rules.max_binary_file_size_bytes, 16 * 1024 * 1024);
}

#[test]
fn multiformat_payloads_normalize_all_supported_extensions() {
    let mut payloads = REQUIRED_CODE_EXTENSIONS
        .iter()
        .map(|extension| required_code_payload(extension))
        .collect::<Vec<_>>();
    payloads.extend(
        REQUIRED_DOC_EXTENSIONS
            .iter()
            .map(|extension| required_doc_payload(extension)),
    );
    payloads.extend(
        REQUIRED_BINARY_EXTENSIONS
            .iter()
            .map(|extension| required_binary_payload(extension)),
    );

    let (nodes, edges) = parse_file_payload_batch(&payloads);

    for payload in &payloads {
        let extension = payload.path.rsplit('.').next().unwrap_or_default();
        let file_node = nodes
            .iter()
            .find(|node| node.id == format!("file:{}", payload.path))
            .unwrap_or_else(|| panic!("missing normalized file node for {}", payload.path));
        let expected_node_type = if REQUIRED_CODE_EXTENSIONS.contains(&extension) {
            "file"
        } else {
            "document"
        };
        assert_eq!(
            file_node.node_type, expected_node_type,
            "wrong node type for {}",
            payload.path
        );
        assert_eq!(
            file_node
                .metadata
                .get("extension")
                .and_then(|value| value.as_str()),
            Some(extension)
        );
        assert!(
            file_node
                .metadata
                .get("repositoryFileKind")
                .and_then(|value| value.as_str())
                .is_some(),
            "missing repositoryFileKind for {}",
            payload.path
        );
    }

    assert!(
        nodes
            .iter()
            .any(|node| node.node_type == "symbol" && node.source_id.ends_with("sample.rs")),
        "expected at least one Rust symbol"
    );
    assert!(
        nodes
            .iter()
            .any(|node| node.node_type == "section" && node.source_id.ends_with("sample.md")),
        "expected markdown section chunks"
    );
    for parser in [
        "docx-ooxml",
        "xlsx-ooxml",
        "pptx-ooxml",
        "pdf-best-effort",
        "image-header",
        "media-header",
    ] {
        assert!(
            nodes.iter().any(
                |node| node.metadata.get("parser").and_then(|value| value.as_str()) == Some(parser)
            ),
            "missing parser metadata for {parser}"
        );
    }
    assert!(
        edges
            .iter()
            .any(|edge| edge.relation == "contains" && edge.evidence == "extracted"),
        "expected extracted containment edges"
    );
}

#[test]
fn repository_query_retrieves_multiformat_content_for_ai_clients() {
    let payloads = vec![
        required_code_payload("rs"),
        required_doc_payload("md"),
        required_binary_payload("docx"),
        required_binary_payload("png"),
        required_binary_payload("wav"),
    ];
    let (nodes, edges) = parse_file_payload_batch(&payloads);
    let graph = KnowledgeGraph {
        version: "test".to_string(),
        generated_at: "2026-05-05T00:00:00Z".to_string(),
        project_id: Uuid::nil(),
        nodes,
        edges,
        communities: Vec::new(),
        statistics: GraphStatistics {
            node_count: 0,
            edge_count: 0,
            community_count: 0,
            evidence_distribution: EvidenceDistribution::default(),
            avg_degree: 0.0,
            density: 0.0,
            isolated_nodes: 0,
        },
    };

    let rust = run_knowledge_query(&graph, "search", None, None, Some("sample.rs"), 1);
    assert!(
        rust.nodes
            .iter()
            .any(|node| node.id == "file:src/sample.rs"),
        "AI-facing repository query should return source references"
    );
    let markdown =
        run_knowledge_query(&graph, "search", None, None, Some("Repository QA Guide"), 1);
    assert!(
        markdown
            .nodes
            .iter()
            .any(|node| node.source_id == "docs/sample.md"),
        "AI-facing repository query should return section references"
    );
    let image = run_knowledge_query(&graph, "search", None, None, Some("sample.png image"), 1);
    let image_node = image
        .nodes
        .iter()
        .find(|node| node.id == "file:assets/sample.png")
        .expect("image query result");
    assert_eq!(
        image_node
            .metadata
            .get("width")
            .and_then(|value| value.as_u64()),
        Some(32)
    );
}

#[test]
fn mixed_batch_failures_emit_diagnostics_without_blocking_healthy_files() {
    let payloads = vec![
        required_code_payload("rs"),
        text_payload(
            "docs/bad.md",
            String::from_utf8_lossy(&[0xff, 0xfe, 0xfd]).to_string(),
        ),
        RepositoryFilePayload {
            path: "docs/invalid-utf8.txt".to_string(),
            content: None,
            bytes: Some(vec![0xff, 0xfe, 0xfd]),
            media_type: Some("text/plain".to_string()),
            size_bytes: Some(3),
        },
        binary_payload(
            "docs/corrupt.docx",
            b"not a zip container".to_vec(),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        binary_payload("assets/corrupt.png", b"not a png".to_vec(), "image/png"),
        text_payload("unknown/sample.abc", "opaque unsupported fixture"),
    ];

    let (nodes, edges) = parse_file_payload_batch(&payloads);

    assert!(
        nodes.iter().any(|node| node.id == "file:src/sample.rs"),
        "healthy file in a mixed batch must still be processed"
    );
    assert!(
        nodes
            .iter()
            .any(|node| node.id == "file:docs/invalid-utf8.txt"),
        "invalid UTF-8 file should produce a metadata-only node"
    );
    assert!(
        nodes.iter().any(|node| node.id == "file:docs/corrupt.docx"),
        "corrupt docx should produce a metadata-only node"
    );
    assert!(
        nodes.iter().any(|node| {
            node.metadata
                .get("diagnostic")
                .and_then(|value| value.as_str())
                .is_some()
        }),
        "expected structured diagnostic nodes"
    );
    assert!(
        edges
            .iter()
            .any(|edge| edge.evidence == "ambiguous" && edge.relation == "contains"),
        "expected ambiguous diagnostic containment edge"
    );
}

#[test]
fn oversized_binary_payload_becomes_metadata_only_without_crashing() {
    let payload = RepositoryFilePayload {
        path: "media/huge.mp4".to_string(),
        content: None,
        bytes: None,
        media_type: Some("video/mp4".to_string()),
        size_bytes: Some(32 * 1024 * 1024),
    };

    let (nodes, edges) = parse_file_payload_batch(&[payload]);

    let file_node = nodes
        .iter()
        .find(|node| node.id == "file:media/huge.mp4")
        .expect("metadata-only oversized media node");
    assert_eq!(
        file_node
            .metadata
            .get("parser")
            .and_then(|value| value.as_str()),
        Some("metadata-only")
    );
    assert!(
        edges
            .iter()
            .any(|edge| edge.evidence == "ambiguous" && edge.source == "file:media/huge.mp4"),
        "oversized binary should include a diagnostic edge"
    );
}
