//! File read tool implementation.

use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::fs;
use tracing::debug;

use crate::{Error, Result, Tool};

/// Maximum file size to read (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// input parameters for the `file_read` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileReadInput {
    /// The path to the file to read.
    path: String,
    /// Optional offset to start reading from (in lines).
    #[serde(default)]
    offset: Option<usize>,
    /// Optional limit on the number of lines to read.
    #[serde(default)]
    limit: Option<usize>,
}

/// A tool for reading file contents.
///
/// This tool reads the contents of a file and returns it as a string.
/// It includes safety checks to prevent directory traversal attacks.
///
/// # Security
///
/// - Paths are validated to prevent directory traversal (`..` components)
/// - File size is limited to prevent memory exhaustion
/// - Only regular files can be read (not symlinks in sensitive locations)
#[derive(Debug, Clone)]
pub struct FileReadTool {
    /// Optional base directory to restrict file access.
    base_dir: Option<PathBuf>,
}

impl Default for FileReadTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FileReadTool {
    /// Creates a new file read tool without path restrictions.
    #[must_use]
    pub fn new() -> Self {
        Self { base_dir: None }
    }

    /// Creates a new file read tool restricted to a base directory.
    ///
    /// All paths will be resolved relative to this directory and
    /// attempts to access files outside will be rejected.
    #[must_use]
    pub fn with_base_dir(base_dir: PathBuf) -> Self {
        Self {
            base_dir: Some(base_dir),
        }
    }

    /// Validates a file path for safety.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The path contains directory traversal components (`..`)
    /// - The path is absolute when a base directory is set
    /// - The path is not a regular file
    fn validate_path(&self, path: &str) -> Result<PathBuf> {
        let path = Path::new(path);

        // Check for directory traversal
        for component in path.components() {
            if matches!(component, Component::ParentDir) {
                return Err(Error::InvalidPath {
                    path: path.to_path_buf(),
                    reason: "directory traversal (..) is not allowed".to_string(),
                });
            }
        }

        // Resolve path relative to base directory if set
        let resolved_path = if let Some(ref base) = self.base_dir {
            if path.is_absolute() {
                return Err(Error::InvalidPath {
                    path: path.to_path_buf(),
                    reason: "absolute paths are not allowed when base directory is set".to_string(),
                });
            }
            base.join(path)
        } else {
            path.to_path_buf()
        };

        Ok(resolved_path)
    }

    /// Checks if the file exists and is within size limits.
    async fn check_file(&self, path: &Path) -> Result<()> {
        let metadata = fs::metadata(path).await.map_err(|e| Error::FileOperation {
            path: path.to_path_buf(),
            message: format!("cannot access file: {e}"),
        })?;

        if !metadata.is_file() {
            return Err(Error::FileOperation {
                path: path.to_path_buf(),
                message: "not a regular file".to_string(),
            });
        }

        if metadata.len() > MAX_FILE_SIZE {
            return Err(Error::FileOperation {
                path: path.to_path_buf(),
                message: format!(
                    "file too large ({} bytes, max {} bytes)",
                    metadata.len(),
                    MAX_FILE_SIZE
                ),
            });
        }

        Ok(())
    }
}

#[async_trait]
impl Tool for FileReadTool {
    fn name(&self) -> &str {
        "file_read"
    }

    fn description(&self) -> &str {
        "Reads the contents of a file. Returns the file content as a string. Supports optional \
         offset and limit parameters to read specific portions of the file."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path to the file to read"
                },
                "offset": {
                    "type": "integer",
                    "description": "Optional line number to start reading from (0-based)",
                    "minimum": 0
                },
                "limit": {
                    "type": "integer",
                    "description": "Optional maximum number of lines to read",
                    "minimum": 1
                }
            },
            "required": ["path"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let input: FileReadInput =
            serde_json::from_value(args).map_err(|e| Error::InvalidArguments {
                tool: self.name().to_string(),
                message: e.to_string(),
            })?;

        debug!("Reading file: {}", input.path);

        let path = self.validate_path(&input.path)?;
        self.check_file(&path).await?;

        let content = fs::read_to_string(&path)
            .await
            .map_err(|e| Error::FileOperation {
                path: path.clone(),
                message: format!("failed to read file: {e}"),
            })?;

        // Apply offset and limit if specified
        let result = if input.offset.is_some() || input.limit.is_some() {
            let lines: Vec<&str> = content.lines().collect();
            let offset = input.offset.unwrap_or(0);
            let limit = input.limit.unwrap_or(lines.len());

            if offset >= lines.len() {
                return Ok(json!({
                    "content": "",
                    "lines": 0,
                    "path": input.path
                }));
            }

            let end = std::cmp::min(offset + limit, lines.len());
            let selected_lines: Vec<&str> = lines[offset..end].to_vec();
            json!({
                "content": selected_lines.join("\n"),
                "lines": selected_lines.len(),
                "path": input.path,
                "offset": offset,
                "total_lines": lines.len()
            })
        } else {
            let lines = content.lines().count();
            json!({
                "content": content,
                "lines": lines,
                "path": input.path
            })
        };

        debug!(
            "Successfully read file: {} ({} lines)",
            input.path, result["lines"]
        );
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;
    use crate::Tool;

    #[test]
    fn test_tool_name() {
        let tool = FileReadTool::new();
        assert_eq!(tool.name(), "file_read");
    }

    #[test]
    fn test_tool_description() {
        let tool = FileReadTool::new();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_input_schema() {
        let tool = FileReadTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("path"))
        );
    }

    #[test]
    fn test_validate_path_traversal() {
        let tool = FileReadTool::new();
        let result = tool.validate_path("../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_path_nested_traversal() {
        let tool = FileReadTool::new();
        let result = tool.validate_path("foo/bar/../../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_path_with_base_dir() {
        let tool = FileReadTool::with_base_dir(PathBuf::from("/safe/dir"));
        let result = tool.validate_path("subdir/file.txt");
        assert!(result.is_ok());
        let path = result.expect("path should be valid");
        assert_eq!(path, PathBuf::from("/safe/dir/subdir/file.txt"));
    }

    #[test]
    fn test_validate_path_absolute_with_base_dir() {
        let tool = FileReadTool::with_base_dir(PathBuf::from("/safe/dir"));
        let result = tool.validate_path("/etc/passwd");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_file_not_found() {
        let tool = FileReadTool::new();
        let args = json!({ "path": "/nonexistent/file.txt" });
        let result = tool.execute(args).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_success() {
        let mut temp_file = NamedTempFile::new().expect("failed to create temp file");
        writeln!(temp_file, "Hello, World!").expect("failed to write");

        let tool = FileReadTool::new();
        let args = json!({ "path": temp_file.path().to_str().unwrap() });
        let result = tool.execute(args).await;

        assert!(result.is_ok());
        let output = result.expect("execute should succeed");
        assert!(
            output["content"]
                .as_str()
                .unwrap()
                .contains("Hello, World!")
        );
    }

    #[tokio::test]
    async fn test_execute_with_offset_and_limit() {
        let mut temp_file = NamedTempFile::new().expect("failed to create temp file");
        for i in 0..10 {
            writeln!(temp_file, "Line {}", i).expect("failed to write");
        }

        let tool = FileReadTool::new();
        let args = json!({
            "path": temp_file.path().to_str().unwrap(),
            "offset": 2,
            "limit": 3
        });
        let result = tool.execute(args).await;

        assert!(result.is_ok());
        let output = result.expect("execute should succeed");
        assert_eq!(output["lines"], 3);
        assert!(output["content"].as_str().unwrap().contains("Line 2"));
        assert!(output["content"].as_str().unwrap().contains("Line 4"));
        assert!(!output["content"].as_str().unwrap().contains("Line 5"));
    }
}
