# Wishlist of Useful Tools

The current assistant has the following built‑in tools:

- `read_file`
- `write_file`
- `edit_file`
- `list_files`
- `search_files`
- `switch_model`
- `run_command` ✅ (already implemented - executes shell commands with user confirmation)

While these are sufficient for basic repository navigation and modification, the following additional tools would make the development workflow smoother and more powerful:

| Desired tool | Why it would help |
|--------------|-------------------|
| **`git_status` / `git_diff`** | Provides insight into the current Git state (what files are staged, what the current HEAD looks like) and shows diffs of edits. This helps reason about version‑control state and avoid accidental overwrites. |
| **`open_file` with line‑number navigation** | Similar to `read_file` but can request a specific line range (e.g., "show me lines 120‑150 of `src/main.rs`"). Makes it easier to focus on a particular region without scrolling through the whole file. |
| **Enhanced `search_files`** – support for multiple patterns or a "search across the whole repo" shortcut | Right now a pattern must be supplied each time; a convenience flag like `pattern: "**/*.rs"` (or simply `all: true`) would let the assistant scan the entire codebase in one call. |

If any of these sound useful, they could be added to the assistant’s toolbox in a future iteration.

Wishlist last updated: $(date)

All listed tools are currently implemented; future enhancements could include `git_status`, `git_diff`, `open_file` with line navigation, and enhanced search across the whole repository.