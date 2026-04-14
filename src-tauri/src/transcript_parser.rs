/// Transcript JSONL parser for Claude Code session files.
///
/// Reads the last N lines of a JSONL transcript to extract:
/// - Context usage (input_tokens + cache_read_input_tokens from last assistant message)
/// - Git branch
/// - Project name (derived from cwd)
/// - Model name
/// - Tool usage status
///
/// Performance note: only the tail of the file is read to avoid loading
/// potentially large transcript files into memory.

use crate::types::*;
use std::fs::File;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;

/// Maximum number of lines to read from the end of the file.
/// Claude Code transcripts grow with each turn; 200 lines is enough
/// to capture the latest assistant message and any running tools.
const MAX_TAIL_LINES: usize = 200;

/// Claude's context window size (200k tokens for Opus/Sonnet).
const CONTEXT_WINDOW_SIZE: u64 = 200_000;

/// Maximum number of tool entries to keep in the result.
/// Prevents unbounded growth when a session uses many tools.
const MAX_TOOLS: usize = 50;

/// Reads a JSONL transcript file and extracts session state from its tail.
///
/// Returns `None` only if the file cannot be opened.
/// Returns `Some(SessionState)` even for empty/partial files — fields
/// that cannot be extracted will have sensible defaults.
pub fn parse_transcript(path: &Path) -> Option<SessionState> {
    let file = File::open(path).ok()?;
    let lines = read_tail_lines(&file, MAX_TAIL_LINES);

    // Accumulators for data extracted while scanning lines
    let mut used_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut model: String = String::new();
    let mut git_branch: Option<String> = None;
    let mut cwd: Option<String> = None;
    let mut session_id: Option<String> = None;
    let mut tools: Vec<ToolInfo> = Vec::new();

    // Walk lines in reverse so the *last* assistant message wins.
    // We iterate from newest (end of file) to oldest.
    for line in lines.iter().rev() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Attempt to parse each line as a TranscriptEntry.
        // Malformed lines are silently skipped — transcript files may
        // contain partial writes or non-JSON lines.
        let mut entry: TranscriptEntry = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        // --- Session ID (first one found in reverse = most recent) ---
        if session_id.is_none() {
            if let Some(ref sid) = entry.session_id {
                session_id = Some(sid.clone());
            }
        }

        // --- Git branch ---
        if git_branch.is_none() {
            git_branch = entry.git_branch.take();
        }

        // --- CWD (working directory) ---
        if cwd.is_none() {
            cwd = entry.cwd.clone();
        }

        // --- Context usage from assistant messages ---
        // The LAST assistant message carries the cumulative token count
        // in its `usage` field. We sum input_tokens + cache_read_input_tokens
        // because cache reads still consume context window space.
        if let Some(msg) = &entry.message {
            if let Some(role) = &msg.role {
                if role == "assistant" {
                    if let Some(usage) = &msg.usage {
                        let input = usage.input_tokens.unwrap_or(0);
                        let cache_read = usage.cache_read_input_tokens.unwrap_or(0);
                        // Only update if we found actual token data
                        let total_input = input + cache_read;
                        if total_input > 0 && used_tokens == 0 {
                            used_tokens = total_input;
                            output_tokens = usage.output_tokens.unwrap_or(0);
                        }
                    }

                    // Extract model from assistant message content if present.
                    // Claude Code embeds the model identifier in the message metadata.
                    if model.is_empty() {
                        if let Some(content) = &msg.content {
                            extract_model_from_content(content, &mut model);
                        }
                    }
                }
            }
        }

        // --- Tool usage tracking ---
        if let Some(ref tool_name_val) = entry.tool_name {
            // Deduplicate: only add if not already tracked
            let tool_name_owned = tool_name_val.clone();
            if !tools.iter().any(|t| t.name == tool_name_owned) {
                let status = determine_tool_status(&entry);
                let detail = extract_tool_detail(&entry);
                tools.push(ToolInfo {
                    name: tool_name_owned,
                    status,
                    detail,
                });
                // Cap the list to avoid unbounded growth
                if tools.len() > MAX_TOOLS {
                    tools.remove(0);
                }
            }
        }
    }

    // Reverse tools so they appear in chronological order
    tools.reverse();

    // Derive project name from cwd: take the last directory component
    let project = cwd
        .as_deref()
        .and_then(|c| Path::new(c).file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    let percentage = if used_tokens > 0 {
        (used_tokens as f64 / CONTEXT_WINDOW_SIZE as f64) * 100.0
    } else {
        0.0
    };

    Some(SessionState {
        session_id: session_id.unwrap_or_default(),
        project,
        cwd: cwd.unwrap_or_default(),
        model,
        git_branch,
        context: ContextInfo {
            used_tokens,
            total_tokens: CONTEXT_WINDOW_SIZE,
            percentage,
        },
        output_tokens,
        tools,
        updated_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
        ide_name: None, // Filled in later by the watcher, not from transcript
    })
}

/// Reads the last `n` lines from a file using backward seeking.
///
/// Strategy:
/// 1. Seek to the end to get file size
/// 2. Read backward in chunks until we have enough lines
/// 3. This avoids reading a multi-MB file from the beginning
fn read_tail_lines(file: &File, n: usize) -> Vec<String> {
    let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
    if file_size == 0 {
        return Vec::new();
    }

    // For small files, just read the whole thing via BufReader
    if file_size < 64 * 1024 {
        let reader = BufReader::new(file);
        let all_lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
        let start = all_lines.len().saturating_sub(n);
        return all_lines[start..].to_vec();
    }

    // For large files, seek near the end and read from there.
    // We read a larger chunk than strictly needed to be sure we get N lines.
    // Each JSONL line is typically 200-2000 bytes, so 200 lines * 2KB = 400KB.
    let seek_back = std::cmp::min(file_size, 512 * 1024); // max 512KB from end
    let seek_pos = file_size - seek_back;

    let mut file = file.try_clone().unwrap_or_else(|_| file.try_clone().unwrap());
    if file.seek(SeekFrom::Start(seek_pos)).is_err() {
        // Fallback: just read from the beginning
        let mut file = file.try_clone().unwrap();
        file.seek(SeekFrom::Start(0)).ok();
        let reader = BufReader::new(file);
        let all_lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();
        let start = all_lines.len().saturating_sub(n);
        return all_lines[start..].to_vec();
    }

    let reader = BufReader::new(file);
    let mut lines: Vec<String> = reader.lines().filter_map(|l| l.ok()).collect();

    // If we didn't seek to position 0, the first line might be partial — discard it
    if seek_pos > 0 && !lines.is_empty() {
        lines.remove(0);
    }

    let start = lines.len().saturating_sub(n);
    lines[start..].to_vec()
}

/// Attempts to extract the model name from assistant message content.
///
/// Claude Code transcripts may include model info in the content array
/// as a text block or in message metadata. We look for a "model" field
/// or try to match common model name patterns in text content.
fn extract_model_from_content(content: &serde_json::Value, model: &mut String) {
    // Check if content is an array of blocks
    if let Some(arr) = content.as_array() {
        for block in arr {
            // Some transcript formats include a "model" key in content blocks
            if let Some(m) = block.get("model").and_then(|v| v.as_str()) {
                *model = m.to_string();
                return;
            }
            // Check nested "text" for model mentions (rare)
            if let Some(text) = block.get("text").and_then(|v| v.as_str()) {
                if text.starts_with("model:") || text.contains("claude-") {
                    // Extract model name from text like "model: claude-sonnet-4-20250514"
                    if let Some(idx) = text.find("claude-") {
                        let rest = &text[idx..];
                        let end = rest.find(|c: char| c.is_whitespace() || c == ',').unwrap_or(rest.len());
                        *model = rest[..end].to_string();
                        return;
                    }
                }
            }
        }
    }

    // Check if content itself has a "model" field (some formats)
    if let Some(m) = content.get("model").and_then(|v| v.as_str()) {
        *model = m.to_string();
    }
}

/// Determines tool status from a transcript entry.
///
/// In Claude Code JSONL:
/// - A tool entry with `tool_input` but no corresponding result yet = Running
/// - The entry type or presence of error fields can indicate Failed
/// - Default assumption for past tool entries = Completed
fn determine_tool_status(entry: &TranscriptEntry) -> ToolStatus {
    // If the entry type suggests it's a tool result with an error, mark as Failed
    if let Some(entry_type) = &entry.entry_type {
        if entry_type == "tool_error" || entry_type == "error" {
            return ToolStatus::Failed;
        }
    }

    // If there's tool_input present, it might be in-progress
    // For a tail-read parser we can't easily distinguish running vs completed,
    // so we default to Completed for entries that have tool_name.
    // The HUD layer can override this with real-time status if needed.
    ToolStatus::Completed
}

/// Extracts a detail string from a tool entry for display purposes.
///
/// For file operations this would be the filename, for searches the query, etc.
fn extract_tool_detail(entry: &TranscriptEntry) -> Option<String> {
    if let Some(input) = &entry.tool_input {
        // Try common tool input fields: "file_path", "command", "query", "pattern"
        for key in &["file_path", "command", "query", "pattern", "path"] {
            if let Some(val) = input.get(*key).and_then(|v| v.as_str()) {
                let detail = val.to_string();
                // Truncate very long details for display
                if detail.len() > 120 {
                    return Some(format!("{}...", &detail[..117]));
                }
                return Some(detail);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    /// Helper: create a temp JSONL file with the given lines
    fn make_temp_jsonl(lines: &[&str]) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp file");
        for line in lines {
            writeln!(f, "{}", line).expect("write line");
        }
        f.flush().expect("flush");
        f
    }

    #[test]
    fn test_parse_empty_file() {
        let mut f = NamedTempFile::new().expect("create temp file");
        f.flush().expect("flush");

        let result = parse_transcript(f.path());
        assert!(result.is_some(), "should return Some even for empty files");

        let state = result.unwrap();
        assert_eq!(state.context.used_tokens, 0, "empty file should have 0 used tokens");
        assert_eq!(state.context.percentage, 0.0, "empty file should have 0% context");
        assert!(state.git_branch.is_none(), "empty file should have no git branch");
        assert!(state.tools.is_empty(), "empty file should have no tools");
    }

    #[test]
    fn test_parse_assistant_usage() {
        // Simulate a transcript with an assistant message carrying usage data.
        // input_tokens=1463, cache_read_input_tokens=77632 => total=79095
        // 79095 / 200000 * 100 = 39.5475%
        let lines = vec![
            r#"{"type":"user","message":{"role":"user","content":"hello"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","usage":{"input_tokens":1463,"cache_read_input_tokens":77632,"output_tokens":621},"content":"hi there"}}"#,
        ];
        let f = make_temp_jsonl(&lines);

        let state = parse_transcript(f.path()).expect("should parse");

        assert_eq!(state.context.used_tokens, 79095);
        assert_eq!(state.context.total_tokens, 200_000);
        let expected_pct = 79095.0 / 200000.0 * 100.0;
        assert!(
            (state.context.percentage - expected_pct).abs() < 0.01,
            "percentage should be ~{:.2} but got {:.2}",
            expected_pct,
            state.context.percentage
        );
        assert_eq!(state.output_tokens, 621);
    }

    #[test]
    fn test_parse_git_branch() {
        let lines = vec![
            r#"{"type":"summary","git_branch":"feat/add-transcript-parser","cwd":"d:/Users/test/project","session_id":"sess-001"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","usage":{"input_tokens":500,"cache_read_input_tokens":0,"output_tokens":100},"content":"ok"}}"#,
        ];
        let f = make_temp_jsonl(&lines);

        let state = parse_transcript(f.path()).expect("should parse");

        assert_eq!(
            state.git_branch.as_deref(),
            Some("feat/add-transcript-parser")
        );
        assert_eq!(state.project, "project");
        assert_eq!(state.session_id, "sess-001");
    }

    #[test]
    fn test_parse_tools() {
        let lines = vec![
            r#"{"type":"tool","tool_name":"Read","tool_input":{"file_path":"src/main.rs"}}"#,
            r#"{"type":"tool","tool_name":"Bash","tool_input":{"command":"cargo test"}}"#,
            r#"{"type":"tool","tool_name":"Grep","tool_input":{"pattern":"TODO","path":"src"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","usage":{"input_tokens":1000,"cache_read_input_tokens":5000,"output_tokens":200},"content":"done"}}"#,
        ];
        let f = make_temp_jsonl(&lines);

        let state = parse_transcript(f.path()).expect("should parse");

        // Should have 3 tools (deduplicated)
        assert_eq!(state.tools.len(), 3);

        // Tools should be in chronological order (Read first)
        assert_eq!(state.tools[0].name, "Read");
        assert_eq!(state.tools[0].detail.as_deref(), Some("src/main.rs"));
        assert!(matches!(state.tools[0].status, ToolStatus::Completed));

        assert_eq!(state.tools[1].name, "Bash");
        assert_eq!(state.tools[1].detail.as_deref(), Some("cargo test"));

        assert_eq!(state.tools[2].name, "Grep");
        assert_eq!(state.tools[2].detail.as_deref(), Some("TODO"));
    }

    #[test]
    fn test_parse_latest_assistant_wins() {
        // When multiple assistant messages exist, the LAST one should be used
        let lines = vec![
            r#"{"type":"assistant","message":{"role":"assistant","usage":{"input_tokens":1000,"cache_read_input_tokens":5000,"output_tokens":100},"content":"first"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","usage":{"input_tokens":2000,"cache_read_input_tokens":10000,"output_tokens":200},"content":"second"}}"#,
        ];
        let f = make_temp_jsonl(&lines);

        let state = parse_transcript(f.path()).expect("should parse");

        // Should use the LAST assistant message: 2000 + 10000 = 12000
        assert_eq!(state.context.used_tokens, 12000);
        assert_eq!(state.output_tokens, 200);
    }

    #[test]
    fn test_parse_tool_deduplication() {
        // Same tool appearing multiple times should be deduplicated
        let lines = vec![
            r#"{"type":"tool","tool_name":"Read","tool_input":{"file_path":"a.rs"}}"#,
            r#"{"type":"tool","tool_name":"Read","tool_input":{"file_path":"b.rs"}}"#,
            r#"{"type":"tool","tool_name":"Read","tool_input":{"file_path":"c.rs"}}"#,
        ];
        let f = make_temp_jsonl(&lines);

        let state = parse_transcript(f.path()).expect("should parse");

        // Should only have 1 Read entry (deduplicated)
        assert_eq!(state.tools.len(), 1);
        // The detail should be from the last occurrence (c.rs) since we walk in reverse
        assert_eq!(state.tools[0].detail.as_deref(), Some("c.rs"));
    }

    #[test]
    fn test_parse_nonexistent_file() {
        let result = parse_transcript(Path::new("/nonexistent/path/transcript.jsonl"));
        assert!(result.is_none(), "nonexistent file should return None");
    }

    #[test]
    fn test_parse_malformed_lines() {
        // File with some garbage lines mixed in should still parse valid entries
        let lines = vec![
            "this is not json",
            "",
            r#"{"type":"assistant","message":{"role":"assistant","usage":{"input_tokens":300,"cache_read_input_tokens":700,"output_tokens":50},"content":"ok"}}"#,
            "more garbage {",
            r#"{"git_branch":"main","cwd":"/home/user/myapp","session_id":"s2"}"#,
        ];
        let f = make_temp_jsonl(&lines);

        let state = parse_transcript(f.path()).expect("should parse");

        assert_eq!(state.context.used_tokens, 1000); // 300 + 700
        assert_eq!(state.git_branch.as_deref(), Some("main"));
        assert_eq!(state.project, "myapp");
    }
}
