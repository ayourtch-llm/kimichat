# Search Files Tool Specification

## 1. Tool name & purpose

**Name:** `search_files`

**Purpose:**
Search for a text string (or regular‑expression) across a set of files in the workspace and return a concise, line‑oriented result set. This gives the models the ability to **discover** information, locate usages, and understand code structure before they read or edit any particular file.

---

## 2. JSON‑schema for arguments

```json
{
  "type": "object",
  "properties": {
    "pattern": {
      "type": "string",
      "description": "Glob pattern (single‑level, no '**') that limits the files to search.  Example: \"src/*.rs\" or \"*.toml\".  Defaults to \"*\" (all files in the workspace).",
      "default": "*"
    },
    "query": {
      "type": "string",
      "description": "The text to look for.  If `regex` is true, this is treated as a regular‑expression."
    },
    "regex": {
      "type": "boolean",
      "description": "If true, interpret `query` as a Rust regex.  If false (default), perform a plain‑text, case‑sensitive substring search.",
      "default": false
    },
    "case_insensitive": {
      "type": "boolean",
      "description": "When `regex` is false, apply case‑insensitive matching.  Ignored when `regex` is true because the regex itself can contain the `(?i)` flag.",
      "default": false
    },
    "max_results": {
      "type": "integer",
      "minimum": 1,
      "description": "Hard cap on the number of matches returned.  Prevents runaway output on very large code bases.  Defaults to 100.",
      "default": 100
    }
  },
  "required": ["query"]
}
```

---

## 3. Human‑readable description (what will be shown to the LLM)

```
search_files: Search for a string or regular‑expression across files matching a glob pattern.

Parameters:
 • pattern (string, optional, default "*"): single‑level glob to limit the files (e.g. "src/*.rs").
 • query (string, required): text to find (or regex when `regex` = true).
 • regex (bool, optional, default false): treat `query` as a Rust regex.
 • case_insensitive (bool, optional, default false): plain‑text search only; ignore case.
 • max_results (int, optional, default 100): maximum number of matches to return.

Result:
A newline‑separated list where each line looks like:
   <relative_path>:<line_number>: <matched_line>

If no matches are found, the tool returns the string "No matches found.".
If the result would exceed `max_results`, the tool stops early and appends
   "... (truncated, N more matches omitted)".
```

---

## 4. Expected Rust implementation (pseudo‑code)

```rust
fn search_files(
    &self,
    pattern: &str,
    query: &str,
    regex: bool,
    case_insensitive: bool,
    max_results: usize,
) -> Result<String> {
    // 1️⃣ Build the glob (single‑level)
    let glob_pat = self.work_dir.join(pattern);
    let mut matches = Vec::new();

    // 2️⃣ Compile the regex if needed
    let re = if regex {
        // Use the `regex` crate; errors are propagated as a friendly message
        Some(Regex::new(query).with_context(|| format!("Invalid regex: {}", query))?)
    } else {
        None
    };

    // 3️⃣ Walk matching files (non‑recursive, respects pattern)
    for entry in glob::glob(glob_pat.to_str().unwrap())? {
        let path = entry?;
        // Skip directories – glob already returns files only in most cases
        let rel_path = path.strip_prefix(&self.work_dir)?.display().to_string();

        // 4️⃣ Read the file line‑by‑line (UTF‑8)
        let file = fs::File::open(&path)?;
        let reader = std::io::BufReader::new(file);
        for (idx, line) in reader.lines().enumerate() {
            let line = line?;
            let is_match = if let Some(re) = &re {
                re.is_match(&line)
            } else {
                if case_insensitive {
                    line.to_lowercase().contains(&query.to_lowercase())
                } else {
                    line.contains(query)
                }
            };

            if is_match {
                matches.push(format!("{}:{}: {}", rel_path, idx + 1, line.trim_end()));
                if matches.len() >= max_results {
                    break;
                }
            }
        }

        if matches.len() >= max_results {
            break;
        }
    }

    // 5️⃣ Build the response string
    if matches.is_empty() {
        Ok("No matches found.".to_string())
    } else {
        let mut out = matches.join("\n");
        // If we stopped early, indicate truncation
        if matches.len() == max_results {
            out.push_str("\n... (truncated, maximum number of results reached)");
        }
        Ok(out)
    }
}
```

**Key safety points (mirroring existing tools):**
* **No recursive globs** – the `pattern` is validated by rejecting any string that contains `"**"` (the `list_files` tool already does this; `search_files` should reuse the same check).
* **Result size guard** – `max_results` caps the number of lines, guaranteeing that the tool never returns a massive payload that could overflow the model’s context window.
* **UTF‑8 only** – Files are read as UTF‑8; binary files will be skipped with a friendly warning (or simply ignored, as they never match a text query anyway).
* **Error handling** – All I/O and regex errors are bubbled up as a clear string (e.g., “Failed to read file X: …”, “Invalid regex …”) so the assistant can surface a useful message to the user.

---

## 5. Integration checklist

1. **Add the tool definition** in `KimiChat::get_tools()` – copy the JSON block above, change `name` to `"search_files"` and `description` to the human‑readable description.
2. **Add a match arm** in `KimiChat::execute_tool`:
   ```rust
   "search_files" => {
       let args: SearchFilesArgs = serde_json::from_str(arguments)?;
       self.search_files(
           &args.pattern,
           &args.query,
           args.regex,
           args.case_insensitive,
           args.max_results as usize,
       )
   }
   ```
3. **Create the argument struct** (just after the other `*Args` structs):
   ```rust
   #[derive(Debug, Deserialize)]
   struct SearchFilesArgs {
       #[serde(default = "default_pattern")]
       pattern: String,
       query: String,
       #[serde(default)]
       regex: bool,
       #[serde(default)]
       case_insensitive: bool,
       #[serde(default = "default_max_results")]
       max_results: u32,
   }

   fn default_max_results() -> u32 { 100 }
   ```
4. **Implement the `search_files` method** (the pseudo‑code above, placed alongside the other file‑operation helpers).
5. **Update tests / manual verification** – run the CLI and try commands like:
   ```
   search_files {"query":"Result","pattern":"src/*.rs","case_insensitive":true}
   ```
   Expected output: lines with “Result” in any `.rs` file under `src/`.

---

## 6. How the two models will use it together

* **Kimi** (fast, coding‑focused) can invoke `search_files` to *quickly locate* a symbol, config key, or TODO comment before opening a file for editing.
* **GPT‑OSS** (deep‑reasoning) can request a *broader pattern* search (e.g., a regex for "unsafe { … }") to analyze architectural concerns, then feed the result back into a higher‑level reasoning step.

Because both models see the exact same tool definition and receive a **deterministic, line‑oriented result**, they can reliably *share* the discovery information in subsequent turns without re‑searching.

---

### ✅ Final spec ready for copy‑paste

```rust
// 1️⃣ Tool definition (add to get_tools())
Tool {
    tool_type: "function".to_string(),
    function: FunctionDef {
        name: "search_files".to_string(),
        description: "Search for a string or regular‑expression across files matching a glob pattern. Returns lines with file:line:content format.".to_string(),
        parameters: serde_json::json!({
            "type": "object",
            "properties": {
                "pattern": {
                    "type": "string",
                    "description": "Single‑level glob pattern (e.g., \"src/*.rs\"). Defaults to \"*\" (all files).",
                    "default": "*"
                },
                "query": {
                    "type": "string",
                    "description": "Text or regex to search for (required)."
                },
                "regex": {
                    "type": "boolean",
                    "description": "Treat `query` as a Rust regex. Default false.",
                    "default": false
                },
                "case_insensitive": {
                    "type": "boolean",
                    "description": "Plain‑text case‑insensitive search (ignored when `regex` is true). Default false.",
                    "default": false
                },
                "max_results": {
                    "type": "integer",
                    "minimum": 1,
                    "description": "Maximum number of matches to return. Default 100.",
                    "default": 100
                }
            },
            "required": ["query"]
        })
    },
},
```

```rust
// 2️⃣ Argument struct
#[derive(Debug, Deserialize)]
struct SearchFilesArgs {
    #[serde(default = "default_pattern")]
    pattern: String,
    query: String,
    #[serde(default)]
    regex: bool,
    #[serde(default)]
    case_insensitive: bool,
    #[serde(default = "default_max_results")]
    max_results: u32,
}
fn default_max_results() -> u32 { 100 }
```

```rust
// 3️⃣ Execution match arm
"search_files" => {
    let args: SearchFilesArgs = serde_json::from_str(arguments)?;
    self.search_files(
        &args.pattern,
        &args.query,
        args.regex,
        args.case_insensitive,
        args.max_results as usize,
    )
}
```

```rust
// 4️⃣ Core implementation (placed with other file helpers)
fn search_files(
    &self,
    pattern: &str,
    query: &str,
    regex: bool,
    case_insensitive: bool,
    max_results: usize,
) -> Result<String> {
    // Guard against recursive patterns
    if pattern.contains("**") {
        return Ok("Recursive patterns (**) are not allowed. Use a single‑level pattern like \"src/*\".".to_string());
    }

    let glob_pat = self.work_dir.join(pattern);
    let mut results = Vec::new();

    // Compile regex if requested
    let re = if regex {
        Some(regex::Regex::new(query)
            .with_context(|| format!("Invalid regex pattern: {}", query))?)
    } else {
        None
    };

    for entry in glob::glob(glob_pat.to_str().unwrap())? {
        let path = entry?;
        let rel_path = path.strip_prefix(&self.work_dir)?.display().to_string();

        let file = fs::File::open(&path)?;
        let reader = std::io::BufReader::new(file);

        for (idx, line) in reader.lines().enumerate() {
            let line = line?;
            let is_match = if let Some(re) = &re {
                re.is_match(&line)
            } else if case_insensitive {
                line.to_lowercase().contains(&query.to_lowercase())
            } else {
                line.contains(query)
            };

            if is_match {
                results.push(format!("{}:{}: {}", rel_path, idx + 1, line.trim_end()));
                if results.len() >= max_results {
                    break;
                }
            }
        }

        if results.len() >= max_results {
            break;
        }
    }

    if results.is_empty() {
        Ok("No matches found.".to_string())
    } else {
        let mut out = results.join("\n");
        if results.len() == max_results {
            out.push_str("\n... (truncated, maximum number of results reached)");
        }
        Ok(out)
    }
}
```

---

With this specification added, **both Kimi and GPT‑OSS** will have a powerful discovery tool that complements the existing read/write/edit/list capabilities, enabling a full *search → read → modify* workflow.
