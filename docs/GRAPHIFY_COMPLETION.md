# Graphify-Aligned Repository Knowledge Graph
### Implemented in Rust with tree-sitter AST parsing and multimodal repository loaders

`chum-mem` now ships two graph layers:

1. The existing **session knowledge graph** built from session ingestion and memory derivation.
2. A new **repository knowledge graph** that mirrors the Graphify-style architecture for repo understanding and exported artifacts.

## Repository graph workflow

The repository graph writes artifacts to `graphify-out/`:

- `graphify-out/graph.json` — persistent graph snapshot
- `graphify-out/graph.nodelink.json` — node-link export
- `graphify-out/GRAPH_REPORT.md` — human-readable report
- `graphify-out/graph.html` — interactive explorer
- `graphify-out/cache/manifest.json` — incremental cache manifest

## Commands

Run from the repo root:

```bash
pnpm kb:build
pnpm kb:update
pnpm kb:cluster
pnpm kb:query -- --text "knowledge graph"
pnpm kb:path -- --from "README" --to "packages/knowledge"
pnpm kb:explain -- --node "apps/api/src/index.ts"
```

## What the repository graph extracts

### Deterministic structural layer

Extracted via tree-sitter AST parsing across 19 languages:
- **Symbols**: functions, classes, structs, traits, interfaces, enums, modules, constants
- **Imports**: language-specific import resolution (ES6 imports, Python imports, Go imports, Rust use, C #include, etc.)
- **Call graph**: function-to-function call edges extracted from AST call expressions
- **Rationale comments**: WHY, NOTE, IMPORTANT, TODO, FIXME tags extracted from AST comment nodes

Supported languages: Python, TypeScript, TSX, JavaScript, Go, Rust, Java, C, C++, Ruby, C#, Kotlin, Scala, PHP, Swift, Lua, Zig, Elixir, Julia

### Multimodal repository layer

The repository sync path now accepts existing text payloads plus optional
`bytesBase64`, `mediaType`, and `sizeBytes` fields for binary files. Binary
payloads are parsed in Rust and normalized into the same `KnowledgeNode` /
`KnowledgeEdge` graph model used by code and text files.

- `.docx` — Office Open XML text extraction from document/header/footer XML
- `.xlsx` — shared string and worksheet text extraction
- `.pptx` — slide text extraction
- `.pdf` — best-effort embedded text extraction with diagnostics when OCR/full parsing is needed
- `.png`, `.jpg`, `.webp`, `.gif` — image metadata and dimensions where decodable
- `.mp4`, `.mov`, `.mp3`, `.wav` — media container/header metadata with transcript-needed markers
- unsupported/corrupt inputs — metadata-only file nodes plus parse diagnostic section nodes

### Semantic/explanatory layer

- markdown and text documents
- sections/headings
- explicit file mentions inside docs
- inferred and ambiguous similarity links between related files/docs

## Runtime graph surfaces

The MCP/API layer now exposes:

- `graph_snapshot`
- `knowledge_graph_export`
- `knowledge_report`
- `knowledge_query`
- `knowledge_communities`

All tools accept an optional `layer` parameter (`"repository"` or `"session"`) to target a specific graph layer.

The web dashboard consumes the real graph snapshot instead of the legacy stub-only nodes/links payload.

## Measured Performance

| Operation | p50 Latency |
|-----------|------------|
| knowledge_report (repository) | 27ms |
| knowledge_query search (repository) | 27ms |
| knowledge_query hub_nodes (repository) | 24ms |
| project_import (20 files, fresh) | 265ms |

## Evidence Distribution

Repository layer (20-file multi-language test corpus):
- Nodes: 171 | Edges: 336 | Communities: 71
- EXTRACTED: 57% (191 edges) — AST-parsed imports, symbols, calls
- INFERRED: 43% (145 edges) — semantic similarity between files
- AMBIGUOUS: 0%
