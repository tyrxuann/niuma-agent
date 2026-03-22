//! Persistence layer for task definitions.
//!
//! This module handles reading and writing task definitions from/to YAML files
//! in the schedules directory.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use niuma_core::Task;
use tokio::{
    fs,
    io::{AsyncReadExt, AsyncWriteExt},
};
use tracing::{debug, info, instrument, warn};

use super::{Error, Result};

/// Handles persistence of task definitions to/from YAML files.
#[derive(Debug)]
pub struct TaskPersistence {
    schedules_dir: PathBuf,
}

impl TaskPersistence {
    /// Creates a new task persistence handler.
    ///
    /// # Arguments
    ///
    /// * `schedules_dir` - The directory where task definitions are stored.
    ///
    /// # Errors
    ///
    /// Returns an error if the schedules directory cannot be created.
    #[instrument(skip_all)]
    pub fn new(schedules_dir: impl Into<PathBuf>) -> Result<Self> {
        let schedules_dir = schedules_dir.into();
        Ok(Self { schedules_dir })
    }

    /// Initializes the persistence layer by creating the schedules directory if needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub async fn init(&self) -> Result<()> {
        if !self.schedules_dir.exists() {
            fs::create_dir_all(&self.schedules_dir).await.map_err(|e| {
                Error::Scheduler(format!("Failed to create schedules directory: {}", e))
            })?;
            debug!(dir = %self.schedules_dir.display(), "Created schedules directory");
        }
        Ok(())
    }

    /// Returns the schedules directory path.
    #[must_use]
    pub fn schedules_dir(&self) -> &Path {
        &self.schedules_dir
    }

    /// Loads all tasks from the schedules directory.
    ///
    /// # Returns
    ///
    /// A map of task IDs to tasks.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be read or tasks cannot be parsed.
    #[instrument(skip(self))]
    pub async fn load_all(&self) -> Result<HashMap<String, Task>> {
        let mut tasks: HashMap<String, Task> = HashMap::new();

        if !self.schedules_dir.exists() {
            info!(
                dir = %self.schedules_dir.display(),
                "Schedules directory does not exist, starting fresh"
            );
            return Ok(tasks);
        }

        let mut entries = fs::read_dir(&self.schedules_dir)
            .await
            .map_err(|e| Error::Scheduler(format!("Failed to read schedules directory: {}", e)))?;

        let mut entry_opt = entries
            .next_entry()
            .await
            .map_err(|e| Error::Scheduler(format!("Failed to read directory entry: {}", e)))?;
        while entry_opt.is_some() {
            if let Some(entry) = entry_opt {
                let path = entry.path();
                if path
                    .extension()
                    .is_some_and(|ext| ext == "yaml" || ext == "yml")
                {
                    match self.load_task_from_file(&path).await {
                        Ok(task) => {
                            debug!(
                                task_id = %task.id,
                                path = %path.display(),
                                "Loaded task"
                            );
                            tasks.insert(task.id.clone(), task);
                        }
                        Err(e) => {
                            warn!(
                                path = %path.display(),
                                error = %e,
                                "Failed to load task from file, skipping"
                            );
                        }
                    }
                }
            }
            entry_opt = entries
                .next_entry()
                .await
                .map_err(|e| Error::Scheduler(format!("Failed to read directory entry: {}", e)))?;
        }

        info!(
            count = tasks.len(),
            dir = %self.schedules_dir.display(),
            "Loaded tasks from disk"
        );
        Ok(tasks)
    }

    /// Loads a single task from a YAML file.
    async fn load_task_from_file(&self, path: &Path) -> Result<Task> {
        let file = fs::File::open(path).await.map_err(|e| {
            Error::Scheduler(format!(
                "Failed to open task file '{}': {}",
                path.display(),
                e
            ))
        })?;

        let mut reader = tokio::io::BufReader::new(file);
        let mut contents = String::new();
        reader.read_to_string(&mut contents).await.map_err(|e| {
            Error::Scheduler(format!(
                "Failed to read task file '{}': {}",
                path.display(),
                e
            ))
        })?;

        let task: Task = serde_yaml::from_str(&contents).map_err(|e| {
            Error::Scheduler(format!(
                "Failed to parse task file '{}': {}",
                path.display(),
                e
            ))
        })?;

        Ok(task)
    }

    /// Saves a task to disk.
    ///
    /// Writes the task to a YAML file named `{task_id}.yaml` in the schedules directory.
    ///
    /// # Arguments
    ///
    /// * `task` - The task to save.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    #[instrument(skip(self, task))]
    pub async fn save(&self, task: &Task) -> Result<()> {
        let path = self.task_path(&task.id);
        self.write_task_to_file(task, &path).await?;
        debug!(task_id = %task.id, path = %path.display(), "Saved task to disk");
        Ok(())
    }

    /// Saves multiple tasks to disk.
    ///
    /// # Arguments
    ///
    /// * `tasks` - The tasks to save.
    ///
    /// # Errors
    ///
    /// Returns an error if any task cannot be written.
    #[instrument(skip(self, tasks))]
    pub async fn save_all(&self, tasks: &HashMap<String, Task>) -> Result<()> {
        for task in tasks.values() {
            self.save(task).await?;
        }
        info!(count = tasks.len(), "Saved all tasks to disk");
        Ok(())
    }

    /// Deletes a task from disk.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The ID of the task to delete.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be deleted.
    #[instrument(skip(self))]
    pub async fn delete(&self, task_id: &str) -> Result<()> {
        let path = self.task_path(task_id);
        if path.exists() {
            fs::remove_file(&path).await.map_err(|e| {
                Error::Scheduler(format!(
                    "Failed to delete task file '{}': {}",
                    path.display(),
                    e
                ))
            })?;
            debug!(
                task_id = %task_id,
                path = %path.display(),
                "Deleted task from disk"
            );
        } else {
            debug!(
                task_id = %task_id,
                "Task file does not exist, nothing to delete"
            );
        }
        Ok(())
    }

    /// Returns the file path for a task.
    fn task_path(&self, task_id: &str) -> PathBuf {
        self.schedules_dir.join(format!("{}.yaml", task_id))
    }

    /// Writes a task to a YAML file.
    async fn write_task_to_file(&self, task: &Task, path: &Path) -> Result<()> {
        let file = fs::File::create(path).await.map_err(|e| {
            Error::Scheduler(format!(
                "Failed to create task file '{}': {}",
                path.display(),
                e
            ))
        })?;

        let yaml_str = serde_yaml::to_string(task).map_err(|e| {
            Error::Scheduler(format!("Failed to serialize task '{}': {}", task.id, e))
        })?;

        let mut writer = tokio::io::BufWriter::new(file);
        writer.write_all(yaml_str.as_bytes()).await.map_err(|e| {
            Error::Scheduler(format!(
                "Failed to write task file '{}': {}",
                path.display(),
                e
            ))
        })?;
        writer.flush().await.map_err(|e| {
            Error::Scheduler(format!(
                "Failed to flush task file '{}': {}",
                path.display(),
                e
            ))
        })?;

        Ok(())
    }

    /// Checks if a task exists on disk.
    #[must_use]
    pub fn exists(&self, task_id: &str) -> bool {
        self.task_path(task_id).exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("Failed to create temp dir")
    }

    #[tokio::test]
    async fn test_persistence_new_creates_dir() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("schedules");
        let persistence = TaskPersistence::new(&schedules_dir);
        assert!(persistence.is_ok());
        // init should create the dir
        persistence
            .unwrap()
            .init()
            .await
            .expect("init should succeed");
        assert!(schedules_dir.exists());
    }

    #[tokio::test]
    async fn test_save_and_load_task() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("schedules");
        let persistence = TaskPersistence::new(&schedules_dir).expect("create should succeed");
        persistence.init().await.expect("init should succeed");

        let task = Task::builder()
            .id("test-task-1")
            .name("Test Task")
            .schedule("0 9 * * *")
            .steps(vec![niuma_core::Step::new(
                "step1",
                "shell",
                serde_json::json!({"command": "echo hello"}),
            )])
            .description("A test task")
            .build()
            .expect("build should succeed");

        persistence.save(&task).await.expect("save should succeed");

        let tasks = persistence.load_all().await.expect("load should succeed");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks.get("test-task-1").unwrap().name, "Test Task");
    }

    #[tokio::test]
    async fn test_load_nonexistent_dir() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("nonexistent");
        let persistence = TaskPersistence::new(&schedules_dir).expect("create should succeed");

        let tasks = persistence.load_all().await.expect("load should succeed");
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn test_delete_task() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("schedules");
        let persistence = TaskPersistence::new(&schedules_dir).expect("create should succeed");
        persistence.init().await.expect("init should succeed");

        let task = Task::builder()
            .id("delete-me")
            .name("Delete Me")
            .schedule("0 9 * * *")
            .build()
            .expect("build should succeed");

        persistence.save(&task).await.expect("save should succeed");
        assert!(persistence.exists("delete-me"));

        persistence
            .delete("delete-me")
            .await
            .expect("delete should succeed");
        assert!(!persistence.exists("delete-me"));
    }

    #[tokio::test]
    async fn test_delete_nonexistent_task() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("schedules");
        let persistence = TaskPersistence::new(&schedules_dir).expect("create should succeed");
        persistence.init().await.expect("init should succeed");

        // Deleting a non-existent task should not error
        let result = persistence.delete("nonexistent").await;
        assert!(result.is_ok());
    }
}
