//! Minimal MCP (Model Context Protocol) client: stdio transport only.
//!
//! Spawns a server process, performs the `initialize` handshake, lists its
//! tools, and calls them. Tools are exposed to the model as
//! `mcp__<server>__<tool>` so they can never collide with built-ins.

use std::collections::HashMap;
use std::process::Stdio;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::{Error, Result};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct McpServerConfig {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
    /// Extra environment variables for the server process.
    pub env: HashMap<String, String>,
    pub enabled: bool,
}

impl Default for McpServerConfig {
    fn default() -> Self {
        Self {
            name: String::new(),
            command: String::new(),
            args: Vec::new(),
            env: HashMap::new(),
            enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

pub const TOOL_PREFIX: &str = "mcp__";

pub fn tool_name(server_id: &str, tool: &str) -> String {
    format!("{TOOL_PREFIX}{server_id}__{tool}")
}

/// Splits `mcp__server__tool` back into its parts.
pub fn parse_tool_name(name: &str) -> Option<(String, String)> {
    let rest = name.strip_prefix(TOOL_PREFIX)?;
    let (server, tool) = rest.split_once("__")?;
    Some((server.to_string(), tool.to_string()))
}

pub struct McpClient {
    server_id: String,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    initialized: bool,
}

impl McpClient {
    /// Spawns the server and completes the initialize handshake.
    pub async fn connect(server_id: &str, config: &McpServerConfig) -> Result<Self> {
        if config.command.trim().is_empty() {
            return Err(Error::other(format!(
                "MCP server \"{server_id}\" has no command"
            )));
        }

        let mut command = Command::new(&config.command);
        // MCP servers are console programs too: without this, connecting one
        // drops an empty black window on the desktop for the app's lifetime.
        crate::process::hide(&mut command);
        command
            .args(&config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            // Dropping the client (a test, a config edit, app shutdown) must
            // kill the server process, not orphan it.
            .kill_on_drop(true);
        for (key, value) in &config.env {
            command.env(key, value);
        }

        let mut child = command.spawn().map_err(|e| {
            Error::other(format!("failed to start MCP server \"{server_id}\": {e}"))
        })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| Error::other("MCP server has no stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| Error::other("MCP server has no stdout"))?;

        let mut client = Self {
            server_id: server_id.to_string(),
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
            initialized: false,
        };

        client
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "clientInfo": { "name": "loom", "version": env!("CARGO_PKG_VERSION") }
                }),
            )
            .await?;

        client
            .notify("notifications/initialized", json!({}))
            .await?;
        client.initialized = true;

        Ok(client)
    }

    pub fn server_id(&self) -> &str {
        &self.server_id
    }

    pub fn is_running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        let message = json!({ "jsonrpc": "2.0", "method": method, "params": params });
        self.write_line(&message).await
    }

    async fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_line(&message).await?;

        // Read until the response with our id arrives (notifications are
        // ignored for now).
        for _ in 0..500 {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .await
                .map_err(|e| Error::other(format!("MCP read failed: {e}")))?;
            if read == 0 {
                return Err(Error::other(format!(
                    "MCP server \"{}\" closed the connection",
                    self.server_id
                )));
            }
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let Ok(value) = serde_json::from_str::<Value>(line) else {
                continue;
            };
            if value.get("id").and_then(Value::as_u64) != Some(id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(Error::other(format!(
                    "MCP error from \"{}\": {}",
                    self.server_id,
                    error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown")
                )));
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }

        Err(Error::other(format!(
            "MCP server \"{}\" did not answer {}",
            self.server_id, method
        )))
    }

    async fn write_line(&mut self, message: &Value) -> Result<()> {
        let mut payload = serde_json::to_string(message)
            .map_err(|e| Error::other(format!("MCP serialisation failed: {e}")))?;
        payload.push('\n');
        self.stdin
            .write_all(payload.as_bytes())
            .await
            .map_err(|e| Error::other(format!("MCP write failed: {e}")))?;
        self.stdin
            .flush()
            .await
            .map_err(|e| Error::other(format!("MCP flush failed: {e}")))
    }

    pub async fn list_tools(&mut self) -> Result<Vec<McpTool>> {
        let result = self.request("tools/list", json!({})).await?;
        let tools = result
            .get("tools")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        Ok(tools
            .into_iter()
            .filter_map(|tool| {
                let name = tool.get("name").and_then(Value::as_str)?.to_string();
                let description = tool
                    .get("description")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                let input_schema = tool
                    .get("inputSchema")
                    .cloned()
                    .unwrap_or_else(|| json!({ "type": "object" }));
                Some(McpTool {
                    name,
                    description,
                    input_schema,
                })
            })
            .collect())
    }

    pub async fn call_tool(&mut self, tool: &str, arguments: Value) -> Result<String> {
        let result = self
            .request(
                "tools/call",
                json!({ "name": tool, "arguments": arguments }),
            )
            .await?;

        // MCP returns content blocks; concatenate the text ones.
        let mut output = String::new();
        if let Some(blocks) = result.get("content").and_then(Value::as_array) {
            for block in blocks {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            output.push_str(text);
                            output.push('\n');
                        }
                    }
                    Some(other) => output.push_str(&format!("[{other} content]\n")),
                    None => {}
                }
            }
        }
        if result.get("isError").and_then(Value::as_bool) == Some(true) {
            return Err(Error::other(if output.is_empty() {
                "MCP tool reported an error".to_string()
            } else {
                output
            }));
        }

        Ok(output.trim_end().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_names_round_trip() {
        let name = tool_name("filesystem", "read_file");
        assert_eq!(name, "mcp__filesystem__read_file");
        assert_eq!(
            parse_tool_name(&name),
            Some(("filesystem".to_string(), "read_file".to_string()))
        );
        assert_eq!(parse_tool_name("read_file"), None);
        assert_eq!(parse_tool_name("mcp__broken"), None);
    }

    #[test]
    fn server_config_defaults_are_sane() {
        let config = McpServerConfig::default();
        assert!(config.enabled);
        assert!(config.args.is_empty());
    }

    /// `test_mcp_server` must fail fast on a bad command, not hang a turn.
    #[tokio::test]
    async fn a_bogus_command_fails_fast_instead_of_hanging() {
        let config = McpServerConfig {
            command: "definitely-not-an-mcp-server".to_string(),
            ..Default::default()
        };
        let started = std::time::Instant::now();
        let result = McpClient::connect("bogus", &config).await;
        assert!(result.is_err());
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "took {:?}",
            started.elapsed()
        );
    }
}
