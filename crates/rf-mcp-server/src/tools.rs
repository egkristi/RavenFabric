//! MCP tool definitions — maps MCP tool calls to RavenFabric operations.

use serde_json::{Value, json};

/// Returns the list of tools this MCP server exposes.
#[allow(clippy::too_many_lines)]
pub fn list_tools() -> Value {
    json!({
        "tools": [
            {
                "name": "rf_exec",
                "description": "Execute a command on the target system. Subject to RavenFabric policy enforcement — commands not matching allow rules will be denied. Immutable deny rules block catastrophic commands regardless of policy. Commands matching approval-required patterns must include an approval_id from a prior rf_request_approval call that has been approved by a human operator.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to execute (e.g., 'ls -la /tmp')"
                        },
                        "workdir": {
                            "type": "string",
                            "description": "Working directory for command execution (optional)"
                        },
                        "reason": {
                            "type": "string",
                            "description": "Explanation of why this command is needed (recorded in audit log)"
                        },
                        "timeout_ms": {
                            "type": "integer",
                            "description": "Execution timeout in milliseconds (optional, default from policy)"
                        },
                        "approval_id": {
                            "type": "string",
                            "description": "Approval ID from rf_request_approval (required for commands matching approval-required patterns)"
                        }
                    },
                    "required": ["command"]
                }
            },
            {
                "name": "rf_query_policy",
                "description": "Check whether a command would be allowed by the current policy without executing it. Returns allow/deny decision and matched rule.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "command": {
                            "type": "string",
                            "description": "The command to check against policy"
                        }
                    },
                    "required": ["command"]
                }
            },
            {
                "name": "rf_file_read",
                "description": "Read a file from the filesystem. Subject to path policy — files outside allowed paths will be denied.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the file to read"
                        }
                    },
                    "required": ["path"]
                }
            },
            {
                "name": "rf_file_write",
                "description": "Write content to a file on the filesystem. Subject to path policy — writes outside allowed paths will be denied.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute path to the file to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "Content to write to the file"
                        },
                        "mode": {
                            "type": "integer",
                            "description": "Unix file permissions (octal, e.g., 0644). Optional."
                        }
                    },
                    "required": ["path", "content"]
                }
            },
            {
                "name": "rf_list_my_capabilities",
                "description": "Discover what commands and paths the current policy allows. Returns allowed command patterns, allowed filesystem paths, and resource limits.",
                "inputSchema": {
                    "type": "object",
                    "properties": {},
                    "required": []
                }
            },
            {
                "name": "rf_audit_query",
                "description": "Query the audit log for recent actions performed in this session. Useful for reviewing what has been done.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of entries to return (default: 20)"
                        },
                        "action_filter": {
                            "type": "string",
                            "description": "Filter by action type (e.g., 'execute', 'read', 'write')"
                        }
                    },
                    "required": []
                }
            },
            {
                "name": "rf_request_approval",
                "description": "Request human approval for a sensitive operation. Blocks until approved or denied by a human operator.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "operation": {
                            "type": "string",
                            "description": "Description of the operation requiring approval"
                        },
                        "command": {
                            "type": "string",
                            "description": "The command that will be executed if approved"
                        },
                        "reason": {
                            "type": "string",
                            "description": "Why this operation is necessary"
                        }
                    },
                    "required": ["operation", "command", "reason"]
                }
            },
            {
                "name": "rf_check_approval",
                "description": "Check the status of a pending approval request. Returns approved, denied, or pending.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "approval_id": {
                            "type": "string",
                            "description": "The approval ID returned by rf_request_approval"
                        }
                    },
                    "required": ["approval_id"]
                }
            }
        ]
    })
}

/// Describes a tool call result as MCP content.
pub fn text_content(text: impl Into<String>) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": text.into()
            }
        ]
    })
}

/// Describes a tool call error result.
pub fn error_content(text: impl Into<String>) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": text.into()
            }
        ],
        "isError": true
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_list_tools_has_all_tools() {
        let tools = list_tools();
        let tools_arr = tools["tools"].as_array().unwrap();
        let names: Vec<&str> = tools_arr
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert!(names.contains(&"rf_exec"));
        assert!(names.contains(&"rf_query_policy"));
        assert!(names.contains(&"rf_file_read"));
        assert!(names.contains(&"rf_file_write"));
        assert!(names.contains(&"rf_list_my_capabilities"));
        assert!(names.contains(&"rf_audit_query"));
        assert!(names.contains(&"rf_request_approval"));
        assert!(names.contains(&"rf_check_approval"));
        assert_eq!(names.len(), 8);
    }

    #[test]
    fn test_text_content_format() {
        let content = text_content("hello world");
        assert_eq!(content["content"][0]["type"], "text");
        assert_eq!(content["content"][0]["text"], "hello world");
    }

    #[test]
    fn test_error_content_has_flag() {
        let content = error_content("something failed");
        assert_eq!(content["isError"], true);
        assert_eq!(content["content"][0]["text"], "something failed");
    }
}
