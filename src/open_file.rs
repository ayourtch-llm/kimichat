use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::ops::RangeInclusive;
use std::fs;
use thiserror::Error;

#[derive(Debug, Error)]
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

const MAX_FILE_SIZE: usize = 1024 * 1024; // 1 MiB

/// Open a file within the given workspace, optionally returning only a line range.
///
/// * `work_dir` – The root workspace directory. The function ensures the resolved file stays inside this directory.
/// * `file_path` – Path relative to the workspace.
/// * `line_range` – Optional inclusive 1‑based line range. If `None`, the whole file is returned.
pub async fn open_file(
    work_dir: &Path,
    file_path: impl AsRef<Path>,
    line_range: Option<RangeInclusive<usize>>,
) -> Result<String> {
    // Resolve the absolute path and ensure it stays within the workspace
    let abs_path = work_dir.join(file_path.as_ref());
    let canonical = abs_path.canonicalize().with_context(|| {
        format!("Failed to canonicalize path: {}", abs_path.display())
    })?;
    let work_canonical = work_dir.canonicalize().with_context(|| {
        format!("Failed to canonicalize workspace dir: {}", work_dir.display())
    })?;
    if !canonical.starts_with(&work_canonical) {
        anyhow::bail!(OpenFileError::PermissionDenied(canonical.display().to_string()));
    }

    // Existence check
    if !canonical.exists() {
        return Err(OpenFileError::FileNotFound(canonical.display().to_string()).into());
    }

    // Size check
    let metadata = fs::metadata(&canonical)?;
    if metadata.len() > MAX_FILE_SIZE as u64 {
        return Err(OpenFileError::FileTooLarge(metadata.len() as usize).into());
    }

    // Read file content as UTF-8
    let raw_bytes = fs::read(&canonical)?;
    let content = String::from_utf8(raw_bytes).map_err(|_| OpenFileError::BinaryFileNotSupported)?;

    // If a line range is requested, slice the lines
    if let Some(range) = line_range {
        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();
        let start = *range.start();
        let end = *range.end();
        if start < 1 || end > total || start > end {
            return Err(OpenFileError::InvalidLineRange { start, end, total_lines: total }.into());
        }
        // Convert to 0-based slice indices (inclusive end)
        let slice = &lines[(start - 1)..end];
        Ok(slice.join("\n"))
    } else {
        Ok(content)
    }
}
