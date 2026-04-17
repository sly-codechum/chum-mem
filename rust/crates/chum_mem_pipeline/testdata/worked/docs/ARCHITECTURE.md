# Architecture

## Overview

This fixture project simulates a realistic multi-language codebase.
The AST pipeline (`chum_mem_pipeline`) parses each file using
tree-sitter grammars and extracts a unified symbol graph.

## Design Decisions

### WHY: Multiple languages in one fixture

Real-world monorepos contain heterogeneous languages. Testing against
a single language would miss edge cases in grammar switching, encoding
detection, and symbol-kind normalization.

### NOTE: File size is intentionally small

Each fixture file is 20-40 lines. This keeps test execution fast while
still exercising every node type the extractor cares about: imports,
definitions, calls, and rationale comments.

### IMPORTANT: Rationale comment format

The pipeline extracts comments matching these patterns:
- `WHY:` - Design rationale
- `NOTE:` - Implementation caveat
- `IMPORTANT:` - Critical constraint

These are embedded in every fixture file so the comment extractor
can be validated across all supported languages.

## Cross-references

- The Python entry point (`src/main.py`) calls into `pipeline.core.Engine`
- The Rust library (`src/lib.rs`) defines the `Extractor` trait used by the pipeline
- The Go runner (`src/main.go`) demonstrates the timeout pattern also used in `src/worker.ex`
