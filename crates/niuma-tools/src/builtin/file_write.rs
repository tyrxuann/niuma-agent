//! File write tool implementation.

use std::path::{Component, Path, PathBuf};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::fs;
use tracing::debug;

use crate::{Error, Result, Tool};

/// Maximum file size to write (10 MB).
const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Input parameters for the `file_write` tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileWriteInput {
    /// The path to the file to write.
    path: String,
    /// The content to write to the file.
    content: String,
    /// Whether to append to the file instead of overwriting.
    #[serde(default)]
    append: bool,
    /// Whether to create parent directories if they don't exist.
    #[serde(default = "default_create_dirs")]
    create_dirs: bool,
}

fn default_create_dirs() -> bool {
    true
}

/// A tool for writing content to files.
///
/// This tool writes content to a file, with options for appending
/// and creating parent directories. It includes safety checks to
/// prevent directory traversal attacks.
///
/// # Security
///
/// - Paths are validated to prevent directory traversal (`..` components)
/// - Content size is limited to prevent disk exhaustion
/// - Paths cannot be absolute when a base directory is set
#[derive(Debug, Clone)]
pub struct FileWriteTool {
    /// Optional base directory to restrict file access.
    base_dir: Option<PathBuf>,
}

impl Default for FileWriteTool {
    fn default() -> Self {
        Self::new()
    }
}

impl FileWriteTool {
    /// Creates a new file write tool without path restrictions.
    #[must_use]
    pub fn new() -> Self {
        Self { base_dir: None }
    }

    /// Creates a new file write tool restricted to a base directory.
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

    /// Validates content size.
    fn validate_content(content: &str) -> Result<()> {
        let size = content.len() as u64;
        if size > MAX_FILE_SIZE {
            return Err(Error::FileOperation {
                path: PathBuf::new(),
                message: format!("content too large ({size} bytes, max {MAX_FILE_SIZE} bytes)"),
            });
        }
        Ok(())
    }
}

#[async_trait]
impl Tool for FileWriteTool {
    fn name(&self) -> &str {
        "file_write"
    }

    fn description(&self) -> &str {
        "Writes content to a file. Can optionally append to existing files and create parent \
         directories. Use with caution as this can overwrite existing files."
    }

    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "path": {
                    "type": "string",
                    "description": "The path to the file to write"
                },
                "content": {
                    "type": "string",
                    "description": "The content to write to the file"
                },
                "append": {
                    "type": "boolean",
                    "description": "Whether to append to the file instead of overwriting (default: false)"
                },
                "create_dirs": {
                    "type": "boolean",
                    "description": "Whether to create parent directories if they don't exist (default: true)"
                }
            },
            "required": ["path", "content"]
        })
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let input: FileWriteInput =
            serde_json::from_value(args).map_err(|e| Error::InvalidArguments {
                tool: self.name().to_string(),
                message: e.to_string(),
            })?;

        debug!("Writing to file: {} (append: {})", input.path, input.append);

        let path = self.validate_path(&input.path)?;
        Self::validate_content(&input.content)?;

        // Create parent directories if requested
        if input.create_dirs
            && let Some(parent) = path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)
                .await
                .map_err(|e| Error::FileOperation {
                    path: parent.to_path_buf(),
                    message: format!("failed to create directories: {e}"),
                })?;
            debug!("Created parent directories: {:?}", parent);
        }

        // Write content
        let bytes_written = if input.append {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await
                .map_err(|e| Error::FileOperation {
                    path: path.clone(),
                    message: format!("failed to open file for appending: {e}"),
                })?;

            use tokio::io::AsyncWriteExt;
            file.write_all(input.content.as_bytes())
                .await
                .map_err(|e| Error::FileOperation {
                    path: path.clone(),
                    message: format!("failed to write to file: {e}"),
                })?;
            file.flush().await.map_err(|e| Error::FileOperation {
                path: path.clone(),
                message: format!("failed to flush file: {e}"),
            })?;

            input.content.len()
        } else {
            fs::write(&path, &input.content)
                .await
                .map_err(|e| Error::FileOperation {
                    path: path.clone(),
                    message: format!("failed to write file: {e}"),
                })?;

            input.content.len()
        };

        debug!(
            "Successfully wrote {} bytes to file: {}",
            bytes_written, input.path
        );

        Ok(json!({
            "success": true,
            "path": input.path,
            "bytes_written": bytes_written
        }))
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::*;
    use crate::Tool;

    #[test]
    fn test_tool_name() {
        let tool = FileWriteTool::new();
        assert_eq!(tool.name(), "file_write");
    }

    #[test]
    fn test_tool_description() {
        let tool = FileWriteTool::new();
        assert!(!tool.description().is_empty());
    }

    #[test]
    fn test_input_schema() {
        let tool = FileWriteTool::new();
        let schema = tool.input_schema();
        assert_eq!(schema["type"], "object");
        assert!(schema["properties"]["path"].is_object());
        assert!(schema["properties"]["content"].is_object());
        let required = schema["required"].as_array().unwrap();
        assert!(required.contains(&json!("path")));
        assert!(required.contains(&json!("content")));
    }

    #[test]
    fn test_validate_path_traversal() {
        let tool = FileWriteTool::new();
        let result = tool.validate_path("../etc/passwd");
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_path_with_base_dir() {
        let tool = FileWriteTool::with_base_dir(PathBuf::from("/safe/dir"));
        let result = tool.validate_path("subdir/file.txt");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_content_too_large() {
        let large_content = "x".repeat((MAX_FILE_SIZE + 1) as usize);
        let result = FileWriteTool::validate_content(&large_content);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_write_new_file() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("test.txt");
        let path_str = file_path.to_str().unwrap();

        let tool = FileWriteTool::new();
        let args = json!({
            "path": path_str,
            "content": "Hello, World!"
        });
        let result = tool.execute(args).await;

        assert!(result.is_ok());
        assert!(file_path.exists());

        let content = fs::read_to_string(&file_path)
            .await
            .expect("failed to read");
        assert_eq!(content, "Hello, World!");
    }

    #[tokio::test]
    async fn test_execute_append() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("test.txt");
        let path_str = file_path.to_str().unwrap();

        let tool = FileWriteTool::new();

        // First write
        let args = json!({
            "path": path_str,
            "content": "First line\n"
        });
        tool.execute(args).await.expect("first write failed");

        // Append
        let args = json!({
            "path": path_str,
            "content": "Second line\n",
            "append": true
        });
        let result = tool.execute(args).await;
        assert!(result.is_ok());

        let content = fs::read_to_string(&file_path)
            .await
            .expect("failed to read");
        assert_eq!(content, "First line\nSecond line\n");
    }

    #[tokio::test]
    async fn test_execute_create_dirs() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("subdir/nested/test.txt");
        let path_str = file_path.to_str().unwrap();

        let tool = FileWriteTool::new();
        let args = json!({
            "path": path_str,
            "content": "Nested content"
        });
        let result = tool.execute(args).await;

        assert!(result.is_ok());
        assert!(file_path.exists());
    }

    #[tokio::test]
    async fn test_execute_no_create_dirs() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let file_path = temp_dir.path().join("nonexistent/test.txt");
        let path_str = file_path.to_str().unwrap();

        let tool = FileWriteTool::new();
        let args = json!({
            "path": path_str,
            "content": "Content",
            "create_dirs": false
        });
        let result = tool.execute(args).await;

        assert!(result.is_err());
    }
}
