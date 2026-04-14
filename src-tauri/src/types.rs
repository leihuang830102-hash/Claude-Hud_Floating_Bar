use serde::{Deserialize, Serialize};

/// Parsed context data extracted from transcript JSONL
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionState {
    pub session_id: String,
    pub project: String,
    pub cwd: String,
    pub model: String,
    pub git_branch: Option<String>,
    pub context: ContextInfo,
    pub output_tokens: u64,
    pub tools: Vec<ToolInfo>,
    pub updated_at: u64,
    pub ide_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextInfo {
    pub used_tokens: u64,
    pub total_tokens: u64,
    pub percentage: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub status: ToolStatus,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ToolStatus {
    Running,
    Completed,
    Failed,
}

/// Parsed from a single line of transcript JSONL
#[derive(Debug, Deserialize)]
pub struct TranscriptEntry {
    #[serde(rename = "type")]
    pub entry_type: Option<String>,
    pub message: Option<MessageData>,
    pub git_branch: Option<String>,
    pub cwd: Option<String>,
    pub session_id: Option<String>,
    pub uuid: Option<String>,
    pub tool_name: Option<String>,
    pub tool_input: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct MessageData {
    pub role: Option<String>,
    pub usage: Option<UsageData>,
    pub content: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct UsageData {
    pub input_tokens: Option<u64>,
    pub cache_read_input_tokens: Option<u64>,
    pub cache_creation_input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// Session metadata from ~/.claude/sessions/<pid>.json
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionMeta {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    pub started_at: u64,
    pub kind: String,
    pub entrypoint: Option<String>,
}

/// IDE connection from ~/.claude/ide/<pid>.lock
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IdeLock {
    pub pid: u32,
    pub workspace_folders: Option<Vec<String>>,
    pub ide_name: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_info_percentage() {
        let ctx = ContextInfo {
            used_tokens: 79095,
            total_tokens: 200000,
            percentage: 39.5,
        };
        assert!((ctx.percentage - 39.5).abs() < 0.01);
    }

    #[test]
    fn test_parse_usage_data() {
        let json = r#"{"input_tokens":1463,"cache_read_input_tokens":77632,"output_tokens":621}"#;
        let usage: UsageData = serde_json::from_str(json).unwrap();
        assert_eq!(usage.input_tokens, Some(1463));
        assert_eq!(usage.cache_read_input_tokens, Some(77632));
        assert_eq!(usage.output_tokens, Some(621));
    }

    #[test]
    fn test_parse_session_meta() {
        let json = r#"{"pid":11868,"sessionId":"abc-123","cwd":"d:\\test","startedAt":1776153780547,"kind":"interactive","entrypoint":"claude-vscode"}"#;
        let meta: SessionMeta = serde_json::from_str(json).unwrap();
        assert_eq!(meta.pid, 11868);
        assert_eq!(meta.cwd, "d:\\test");
    }

    #[test]
    fn test_parse_ide_lock() {
        let json = r#"{"pid":4252,"workspaceFolders":["d:\\Users\\test\\project"],"ideName":"Trae CN","transport":"ws"}"#;
        let lock: IdeLock = serde_json::from_str(json).unwrap();
        assert_eq!(lock.pid, 4252);
        assert_eq!(lock.ide_name, Some("Trae CN".to_string()));
    }
}
