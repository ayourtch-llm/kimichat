# Enhanced search_files Tool Specification

<!--
## Review Comments (GPT‑OSS)

### Overall Structure & Clarity
- Strengths: well‑organized headings, clear usage examples.
- Suggestion: add a concise **Signature** block showing the full function signature after enhancements.
- Keep code‑block language consistent (prefer `rust`).

### New Parameters
| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `all` | `bool` | `false` | When true, scans the entire repository regardless of pattern.
| `pattern` | `&str` (String) | `"*"` | Can accept comma‑separated globs or shortcuts (`"all"`). |
- Note mutual exclusivity: `all:true` makes `pattern` optional/ignored.

### Performance Considerations
- Suggest optional parallel traversal (e.g., Rayon).
- Mention possible caching of file‑list results.
- Provide `max_concurrent_reads` to throttle I/O.

### Backward Compatibility
- Old call `search_files("TODO")` maps to `search_files("TODO", pattern="*")`.
- Existing single‑pattern usage continues unchanged.

### Return Format
- Keep `file:line:content` ordering.
- Optionally expose which pattern matched (future).

### Example Use‑Case (all features)
```rust
search_files(
    query: r"TODO|FIXME|HACK",
    pattern: "src/**/*.rs,src/**/*.py,src/**/*.js",
    regex: true,
    max_results: 200,
);
```

### Future Enhancements
- Negated patterns (e.g., `!**/test/*`).
- Directory exclusions (`exclude_dirs`).
-->

## Overview

This specification outlines enhancements to the existing `search_files` tool to support whole-repository scanning and multiple pattern convenience features.

## Current Limitations

- Requires explicit pattern specification for each search
- No convenient way to scan entire repository
- Pattern must be supplied manually each time

## Proposed Enhancements

### 1. Whole-Repository Scanning

**New parameter:**
```rust
all: bool  // When true, scans entire repository regardless of pattern
```

**Usage examples:**
```rust
search_files("TODO", all: true)           // Scan entire repo for TODO
search_files("TODO", all: true, max_results: 50)  // Limit results
```

### 2. Multiple Pattern Support

**Enhanced parameter:**
```rust
pattern: &str  // Can now accept comma-separated patterns or special values
```

**Pattern shortcuts:**
- `"**/*.rs"` - All Rust files recursively
- `"src/**/*.py"` - All Python in src directory
- `"all"` - Entire repository (equivalent to all: true)
- `"*.rs,*.py,*.js"` - Multiple extensions

### 3. Enhanced Features

**Improved search capabilities:**
- Recursive directory traversal
- Multiple pattern matching
- File type filtering
- Directory-based searching
- Glob pattern support

**Usage examples:**
```rust
// Search entire codebase
search_files("TODO", pattern: "all")

// Multiple file types
search_files("TODO", pattern: "*.rs,*.py,*.js")

// Recursive directory
search_files("TODO", pattern: "src/**/*.rs")

// Combined with regex
search_files(r"TODO|FIXME|HACK", pattern: "all", regex: true)
```

### 4. Performance Considerations

**Implementation notes:**
- Efficient directory traversal
- Pattern matching optimization
- Result limiting for large repositories
- Memory-conscious scanning

## Benefits

**Development workflow improvements:**
- Faster codebase exploration
- Comprehensive repository scanning
- Multiple pattern convenience
- Whole-repository awareness

**Example scenarios:**
```rust
// Find all TODO comments across entire codebase
search_files("TODO", all: true)

// Scan specific file types
search_files("TODO", pattern: "*.rs,*.py")

// Recursive directory search
search_files("TODO", pattern: "src/**/*.rs")
```

## Technical Implementation

**Core functionality:**
- Directory traversal with glob patterns
- Multiple pattern matching
- Recursive repository scanning
- Performance optimization
- Memory-efficient operation

**Integration approach:**
- Extend existing search_files tool
- Maintain backward compatibility
- Add convenience parameters
- Preserve current functionality

## Future Considerations

**Potential enhancements:**
- File type filtering
- Directory-specific searching
- Pattern combination
- Performance monitoring
- Large repository optimization

## Summary

Enhanced search_files tool provides whole-repository scanning capabilities, multiple pattern support, and comprehensive search functionality. This addresses current repository exploration limitations while maintaining existing functionality and performance characteristics.