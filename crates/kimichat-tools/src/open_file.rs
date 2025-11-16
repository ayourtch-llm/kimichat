use anyhow::{Context, Result};
use std::path::Path;
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
    // Resolve the absolute path
    let abs_path = work_dir.join(file_path.as_ref());

    // Check if the exact path exists before trying to canonicalize
    if !abs_path.exists() {
        // Provide helpful error message if there's a similar directory
        let file_name = abs_path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // Check for directory with name matching the file without extension
        if let Some(stem) = abs_path.file_stem().and_then(|s| s.to_str()) {
            let parent = abs_path.parent().unwrap_or(work_dir);
            let possible_dir = parent.join(stem);

            if possible_dir.exists() && possible_dir.is_dir() {
                return Err(OpenFileError::FileNotFound(format!(
                    "{} (Note: Found a directory named '{}' at this location. Did you mean to list files in that directory instead?)",
                    abs_path.display(),
                    stem
                )).into());
            }
        }

        // Check if parent directory exists
        if let Some(parent) = abs_path.parent() {
            if !parent.exists() {
                return Err(OpenFileError::FileNotFound(format!(
                    "{} (parent directory '{}' does not exist)",
                    abs_path.display(),
                    parent.display()
                )).into());
            }
        }

        return Err(OpenFileError::FileNotFound(abs_path.display().to_string()).into());
    }

    // Now canonicalize the existing path
    let canonical = abs_path.canonicalize().with_context(|| {
        format!("Failed to canonicalize path: {}", abs_path.display())
    })?;

    let work_canonical = work_dir.canonicalize().with_context(|| {
        format!("Failed to canonicalize workspace dir: {}", work_dir.display())
    })?;

    if !canonical.starts_with(&work_canonical) {
        anyhow::bail!(OpenFileError::PermissionDenied(canonical.display().to_string()));
    }

    // Check if it's a directory instead of a file
    if canonical.is_dir() {
        return Err(OpenFileError::FileNotFound(format!(
            "{} is a directory, not a file. Use list_files to see its contents.",
            canonical.display()
        )).into());
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
        let requested_start = *range.start();
        let requested_end = *range.end();

        // Clamp the range to valid values instead of failing
        let start = requested_start.max(1).min(total.max(1));
        let end = requested_end.min(total).max(start);

        // If the file is empty, return empty string
        if total == 0 {
            return Ok(String::new());
        }

        // Convert to 0-based slice indices (inclusive end)
        let slice = &lines[(start - 1)..end];
        Ok(slice.join("\n"))
    } else {
        Ok(content)
    }
}
