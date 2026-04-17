# Worked Example Project

This is a synthetic multi-language project used as a test fixture for
tree-sitter AST extraction validation.

## Structure

- `src/main.py` - Python pipeline entry point
- `src/app.ts` - TypeScript Express application
- `src/component.tsx` - React profile card component
- `src/server.js` - Node.js HTTP server with event bus
- `src/main.go` - Go task runner with timeout support
- `src/lib.rs` - Rust document/symbol extraction library
- `src/App.java` - Java plugin-based application
- `src/utils.c` - C buffer utilities
- `src/engine.cpp` - C++ compute engine with backend abstraction
- `src/handler.rb` - Ruby request handler
- `src/Service.cs` - C# data service with interface
- `src/Parser.kt` - Kotlin config file parser
- `src/Processor.scala` - Scala batch event processor
- `src/index.php` - PHP application with routing

## Purpose

Each file exercises common AST patterns: imports, symbol definitions
(classes, structs, functions, interfaces), function calls, and
rationale comments (`WHY:`, `NOTE:`, `IMPORTANT:`).

See `docs/ARCHITECTURE.md` for design decisions.
