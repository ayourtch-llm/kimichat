use std::path::Path;

/// Build a glob pattern string from a pattern and work directory.
/// If the pattern is absolute, returns it as-is.
/// If relative, joins it with the work directory.
pub fn build_glob_pattern(pattern: &str, work_dir: &Path) -> String {
    if Path::new(pattern).is_absolute() {
        pattern.to_string()
    } else {
        work_dir.join(pattern)
            .to_string_lossy()
            .to_string()
    }
}
