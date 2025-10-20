# open_file Tool Specification

## Overview

The `open_file` tool provides **line‑number navigation** for reading files. It builds on the existing `read_file` tool but adds the ability to request a specific line range, making it easier to focus on a particular region of a source file.

## Purpose

- View only the part of a file you need (e.g., lines 120‑150 of `src/main.rs`).
- Reduce token usage and scrolling when dealing with large files.
- Support precise code‑review and debugging workflows.
- Remain consistent with the rest of the file‑operation toolbox.

## Specification

### Function Signature
```rust
pub async fn open_file<P: AsRef<std::path::Path>>(
    file_path: P,
    line_range: Option<std::ops::RangeInclusive<usize>>, // 1‑indexed, inclusive
) -> anyhow::Result<String>
```

### Parameters
- **`file_path`** – Path to the file **relative to the workspace**. The function must resolve the path safely so that it cannot escape the workspace directory.
- **`line_range`** – Optional inclusive range (`start..=end`). If `None`, the whole file is returned.

### Behavior
- **Line Navigation** – When a range is supplied, the tool returns **exactly** the lines within that inclusive range, preserving the original line endings.
- **Whole‑file fallback** – If `line_range` is `None`, the complete file content is returned.
- **Safety** – The implementation must:
  1. Verify that the resolved path stays inside the workspace.
  2. Enforce a maximum file size (e.g., 1 MiB) to avoid sending huge binaries.
  3. Detect non‑UTF‑8 files and return a clear `BinaryFileNotSupported` error.
- **Error Handling** – Returns detailed errors for:
  * `FileNotFound`
  * `PermissionDenied`
  * `InvalidLineRange { start, end, total_lines }`
  * `FileTooLarge`
  * `BinaryFileNotSupported`
  * General I/O errors (wrapped in `anyhow`).
- **Integration** – Internally re‑uses the existing `read_file` implementation when `line_range` is `None` to avoid code duplication.

### Example Usage
```rust
// Show lines 120‑150 (inclusive)
let snippet = open_file("src/main.rs", Some(120..=150)).await?;

// Show the entire file
let whole = open_file("src/main.rs", None).await?;
```

## Implementation Notes

### Line Numbering
- Users think in **1‑indexed** line numbers, matching typical editor conventions.
- The inclusive range (`RangeInclusive`) mirrors the phrasing "lines 120‑150".

### Efficient Reading
- For large files, read the file **line‑by‑line** and stop once the requested range is collected, rather than loading the entire file into memory.
- When the whole file is requested, fall back to `tokio::fs::read_to_string` (subject to the size limit).

### Security & Validation
- **Workspace confinement** – Resolve the path and ensure it starts with the workspace root.
- **Size limit** – Reject files larger than a configurable threshold (default 1 MiB) with `FileTooLarge`.
- **Binary detection** – Attempt `String::from_utf8`; on failure, return `BinaryFileNotSupported`.

### Error Types (illustrative)
```rust
#[derive(Debug, thiserror::Error)]
pub enum OpenFileError {
    #[error("file not found: {0}")]
    FileNotFound(String),
    #[error("permission denied: {0}")]
    PermissionDenied(String),
    #[error("invalid line range {start}..={end} for a file with {total_lines} lines")]
    InvalidLineRange { start: usize, end: usize, total_lines: usize },
    #[error("file exceeds maximum allowed size of {0} bytes")]
    FileTooLarge(usize),
    #[error("binary file cannot be displayed as text")]
    BinaryFileNotSupported,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
```
These can be wrapped in `anyhow::Error` for the public API.

## Benefits

- **Precise Navigation** – Jump directly to the region of interest.
- **Efficiency** – Smaller payloads when only a fragment is needed.
- **Improved UX** – Reduces the need for the assistant to request the whole file and then slice it.
- **Consistent Security** – Works within the same sandboxed workspace model as other tools.

## Future Considerations

- **Context Lines** – Optionally include `n` lines before/after the requested range for better context.
- **Line Number Prefixes** – Add a flag to prepend line numbers to each returned line.
- **Highlighting** – Return the snippet with ANSI colour codes for easier terminal reading.
- **Caching** – Simple in‑memory cache for recently opened files (size‑bounded).
- **Version‑control Integration** – Combine with future `git_diff`/`git_status` tools to show changes for the selected range.
