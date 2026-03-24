//! Task scheduler for cron-based task execution.
//!
//! This module provides the [`TaskScheduler`] which manages scheduled tasks,
//! loads them from disk at startup, persists changes, and executes tasks
//! when their cron schedule triggers.

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use niuma_core::{Step, Task};
use tokio::sync::{Mutex, RwLock};
use tokio_cron_scheduler::{Job, JobScheduler};
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;

use super::{Error, Result, persistence::TaskPersistence};

/// A task scheduler that manages cron-based scheduled tasks.
///
/// The scheduler loads tasks from disk at startup, persists changes
/// automatically, and executes tasks when their cron schedule triggers.
pub struct TaskScheduler {
    inner: Arc<Mutex<JobScheduler>>,
    tasks: Arc<RwLock<HashMap<String, Task>>>,
    /// Maps task IDs to their corresponding job UUIDs in the scheduler.
    job_guids: Arc<RwLock<HashMap<String, Uuid>>>,
    persistence: TaskPersistence,
    shutdown: Arc<AtomicBool>,
    executor: Arc<dyn TaskExecutor>,
}

impl std::fmt::Debug for TaskScheduler {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let task_count = self.tasks.try_read().map_or(0, |g| g.len());
        let job_count = self.job_guids.try_read().map_or(0, |g| g.len());
        f.debug_struct("TaskScheduler")
            .field("task_count", &task_count)
            .field("job_count", &job_count)
            .finish()
    }
}

/// Trait for executing tasks.
///
/// This trait allows the scheduler to delegate task execution to
/// an external component (e.g., the [`Executor`](super::Executor)).
pub trait TaskExecutor: Send + Sync {
    /// Executes a task's steps.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The unique identifier of the task.
    /// * `task_name` - The human-readable name of the task.
    /// * `steps` - The pre-confirmed steps to execute.
    fn execute_task(&self, task_id: &str, task_name: &str, steps: Vec<Step>);
}

/// A no-op task executor for testing or when actual execution is not needed.
#[derive(Debug)]
pub struct NoopExecutor;

impl TaskExecutor for NoopExecutor {
    fn execute_task(&self, task_id: &str, task_name: &str, _steps: Vec<Step>) {
        debug!(
            task_id = %task_id,
            task_name = %task_name,
            "NoopExecutor: task would execute here"
        );
    }
}

impl TaskScheduler {
    /// Creates a new task scheduler.
    ///
    /// # Arguments
    ///
    /// * `schedules_dir` - The directory where task definitions are stored.
    /// * `executor` - The task executor to use for running tasks.
    ///
    /// # Errors
    ///
    /// Returns an error if the scheduler cannot be initialized or tasks
    /// cannot be loaded.
    #[instrument(skip_all)]
    pub async fn new(
        schedules_dir: impl Into<PathBuf>,
        executor: Arc<dyn TaskExecutor>,
    ) -> Result<Self> {
        let schedules_dir = schedules_dir.into();

        let persistence = TaskPersistence::new(&schedules_dir)?;
        persistence.init().await?;
        let tasks = persistence.load_all().await?;

        let inner = JobScheduler::new()
            .await
            .map_err(|e| Error::Scheduler(format!("Failed to create job scheduler: {}", e)))?;

        let scheduler = Self {
            inner: Arc::new(Mutex::new(inner)),
            tasks: Arc::new(RwLock::new(tasks)),
            job_guids: Arc::new(RwLock::new(HashMap::new())),
            persistence,
            shutdown: Arc::new(AtomicBool::new(false)),
            executor,
        };

        info!(
            task_count = scheduler.tasks.try_read().map(|t| t.len()).unwrap_or(0),
            dir = %schedules_dir.display(),
            "Task scheduler initialized"
        );

        Ok(scheduler)
    }

    /// Starts the scheduler.
    ///
    /// This begins processing cron schedules. Call [`shutdown`](Self::shutdown) to stop.
    ///
    /// # Errors
    ///
    /// Returns an error if the scheduler fails to start.
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<()> {
        // Schedule all currently loaded tasks
        let tasks_snapshot: Vec<Task> = {
            let tasks = self.tasks.read().await;
            tasks.values().cloned().collect()
        };

        for task in tasks_snapshot {
            if task.enabled
                && let Err(e) = self.schedule_task_internal(&task).await
            {
                warn!(task_id = %task.id, error = %e, "Failed to schedule task at startup");
            }
        }

        self.inner
            .lock()
            .await
            .start()
            .await
            .map_err(|e| Error::Scheduler(format!("Failed to start job scheduler: {}", e)))?;

        info!("Task scheduler started");
        Ok(())
    }

    /// Shuts down the scheduler gracefully.
    ///
    /// Stops accepting new executions and waits for running tasks to complete.
    ///
    /// # Errors
    ///
    /// Returns an error if the scheduler fails to shut down.
    pub async fn shutdown(&self) -> Result<()> {
        self.shutdown.store(true, Ordering::SeqCst);

        self.inner
            .lock()
            .await
            .shutdown()
            .await
            .map_err(|e| Error::Scheduler(format!("Failed to shutdown job scheduler: {}", e)))?;

        info!("Task scheduler shut down");
        Ok(())
    }

    /// Returns the number of tasks currently managed by the scheduler.
    #[must_use]
    pub async fn task_count(&self) -> usize {
        self.tasks.read().await.len()
    }

    /// Returns a list of all tasks.
    #[must_use]
    pub async fn list_tasks(&self) -> Vec<Task> {
        let tasks = self.tasks.read().await;
        tasks.values().cloned().collect()
    }

    /// Returns a task by ID.
    ///
    /// # Arguments
    ///
    /// * `task_id` - The ID of the task.
    #[must_use]
    pub async fn get_task(&self, task_id: &str) -> Option<Task> {
        let tasks = self.tasks.read().await;
        tasks.get(task_id).cloned()
    }

    /// Adds a new task.
    ///
    /// The task is saved to disk and scheduled for execution.
    ///
    /// # Errors
    ///
    /// Returns an error if the task cannot be saved or scheduled.
    #[instrument(skip(self, task))]
    pub async fn add_task(&self, task: Task) -> Result<()> {
        let task_id = task.id.clone();

        // Save to disk
        self.persistence.save(&task).await?;

        // Add to in-memory map
        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id.clone(), task.clone());
        }

        // Schedule the task if enabled
        if task.enabled {
            self.schedule_task_internal(&task).await?;
        }

        info!(task_id = %task_id, "Task added and scheduled");
        Ok(())
    }

    /// Removes a task by ID.
    ///
    /// The task is removed from the scheduler and deleted from disk.
    ///
    /// # Errors
    ///
    /// Returns an error if the task cannot be removed.
    #[instrument(skip(self))]
    pub async fn remove_task(&self, task_id: &str) -> Result<()> {
        // Remove from in-memory map
        let removed = {
            let mut tasks = self.tasks.write().await;
            tasks.remove(task_id)
        };

        if removed.is_none() {
            return Err(Error::Scheduler(format!("Task '{}' not found", task_id)));
        }

        // Remove scheduled job
        self.unschedule_task(task_id).await?;

        // Delete from disk
        self.persistence.delete(task_id).await?;

        info!(task_id = %task_id, "Task removed");
        Ok(())
    }

    /// Updates an existing task.
    ///
    /// The task is saved to disk and rescheduled.
    ///
    /// # Errors
    ///
    /// Returns an error if the task cannot be updated.
    #[instrument(skip(self, task))]
    pub async fn update_task(&self, task: Task) -> Result<()> {
        let task_id = task.id.clone();

        // Check task exists
        if !self.tasks.read().await.contains_key(&task_id) {
            return Err(Error::Scheduler(format!("Task '{}' not found", task_id)));
        }

        // Unschedule existing job
        self.unschedule_task(&task_id).await?;

        // Save to disk
        self.persistence.save(&task).await?;

        // Update in-memory map
        {
            let mut tasks = self.tasks.write().await;
            tasks.insert(task_id.clone(), task.clone());
        }

        // Reschedule if enabled
        if task.enabled {
            self.schedule_task_internal(&task).await?;
        }

        info!(task_id = %task_id, "Task updated");
        Ok(())
    }

    /// Enables a task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task is not found or cannot be enabled.
    #[instrument(skip(self))]
    pub async fn enable_task(&self, task_id: &str) -> Result<()> {
        let task = {
            let mut tasks = self.tasks.write().await;
            let task = tasks
                .get_mut(task_id)
                .ok_or_else(|| Error::Scheduler(format!("Task '{}' not found", task_id)))?;

            if task.enabled {
                debug!(task_id = %task_id, "Task already enabled");
                return Ok(());
            }

            task.enabled = true;
            task.clone()
        };

        self.persistence.save(&task).await?;
        self.schedule_task_internal(&task).await?;

        info!(task_id = %task_id, "Task enabled");
        Ok(())
    }

    /// Disables a task.
    ///
    /// # Errors
    ///
    /// Returns an error if the task is not found or cannot be disabled.
    #[instrument(skip(self))]
    pub async fn disable_task(&self, task_id: &str) -> Result<()> {
        let task = {
            let mut tasks = self.tasks.write().await;
            let task = tasks
                .get_mut(task_id)
                .ok_or_else(|| Error::Scheduler(format!("Task '{}' not found", task_id)))?;

            if !task.enabled {
                debug!(task_id = %task_id, "Task already disabled");
                return Ok(());
            }

            task.enabled = false;
            task.clone()
        };

        self.persistence.save(&task).await?;
        self.unschedule_task(task_id).await?;

        info!(task_id = %task_id, "Task disabled");
        Ok(())
    }

    /// Manually triggers a task by ID.
    ///
    /// # Errors
    ///
    /// Returns an error if the task is not found.
    #[instrument(skip(self))]
    pub async fn run_task(&self, task_id: &str) -> Result<()> {
        let task = self
            .get_task(task_id)
            .await
            .ok_or_else(|| Error::Scheduler(format!("Task '{}' not found", task_id)))?;

        self.execute_task(&task).await;
        Ok(())
    }

    /// Schedules a task internally (adds the cron job to the scheduler).
    async fn schedule_task_internal(&self, task: &Task) -> Result<()> {
        let task_id = task.id.clone();
        let task_name = task.name.clone();
        let steps = task.steps.clone();
        let tasks = Arc::clone(&self.tasks);
        let executor = Arc::clone(&self.executor);
        let shutdown = Arc::clone(&self.shutdown);

        let job = Job::new_async(&task.schedule, move |_uuid, _lock| {
            let task_id = task_id.clone();
            let task_name = task_name.clone();
            let steps = steps.clone();
            let tasks = Arc::clone(&tasks);
            let executor = Arc::clone(&executor);
            let shutdown = Arc::clone(&shutdown);

            Box::pin(async move {
                if shutdown.load(Ordering::SeqCst) {
                    debug!(task_id = %task_id, "Skipping task execution due to shutdown");
                    return;
                }

                // Re-check if task is still enabled
                let should_run = {
                    let tasks_guard = tasks.read().await;
                    tasks_guard.get(&task_id).is_some_and(|t| t.enabled)
                };

                if !should_run {
                    debug!(task_id = %task_id, "Task is disabled, skipping execution");
                    return;
                }

                debug!(
                    task_id = %task_id,
                    task_name = %task_name,
                    "Cron trigger fired for task"
                );
                executor.execute_task(&task_id, &task_name, steps);
            })
        })
        .map_err(|e| {
            Error::Scheduler(format!(
                "Invalid cron expression '{}': {}",
                task.schedule, e
            ))
        })?;

        // Get the job UUID before adding
        let job_uuid = job.guid();

        self.inner.lock().await.add(job).await.map_err(|e| {
            Error::Scheduler(format!("Failed to add job for task '{}': {}", task.id, e))
        })?;

        // Track the job UUID
        {
            let mut guids = self.job_guids.write().await;
            guids.insert(task.id.clone(), job_uuid);
        }

        debug!(
            task_id = %task.id,
            schedule = %task.schedule,
            "Task scheduled"
        );
        Ok(())
    }

    /// Unschedules a task (removes the cron job from the scheduler).
    async fn unschedule_task(&self, task_id: &str) -> Result<()> {
        let job_uuid = {
            let guids = self.job_guids.read().await;
            guids.get(task_id).copied()
        };

        if let Some(uuid) = job_uuid {
            self.inner.lock().await.remove(&uuid).await.map_err(|e| {
                Error::Scheduler(format!(
                    "Failed to remove job for task '{}': {}",
                    task_id, e
                ))
            })?;
            debug!(task_id = %task_id, "Job unscheduled");
        }

        // Remove from tracking map
        {
            let mut guids = self.job_guids.write().await;
            guids.remove(task_id);
        }

        Ok(())
    }

    /// Executes a task using the configured executor.
    async fn execute_task(&self, task: &Task) {
        info!(
            task_id = %task.id,
            task_name = %task.name,
            step_count = task.steps.len(),
            "Executing scheduled task"
        );

        self.executor
            .execute_task(&task.id, &task.name, task.steps.clone());
    }
}

/// Configuration for storage paths.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Directory for task schedule definitions.
    pub schedules_dir: PathBuf,
    /// Directory for execution plan cache.
    pub cache_dir: PathBuf,
    /// Directory for execution logs.
    pub logs_dir: PathBuf,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            schedules_dir: PathBuf::from("./data/schedules"),
            cache_dir: PathBuf::from("./data/cache/plans"),
            logs_dir: PathBuf::from("./data/logs"),
        }
    }
}

impl StorageConfig {
    /// Creates a new storage config with base directory.
    ///
    /// All subdirectories are created relative to the base directory.
    #[must_use]
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        let base = base_dir.into();
        Self {
            schedules_dir: base.join("schedules"),
            cache_dir: base.join("cache/plans"),
            logs_dir: base.join("logs"),
        }
    }

    /// Creates all storage directories.
    ///
    /// # Errors
    ///
    /// Returns an error if any directory cannot be created.
    pub async fn create_dirs(&self) -> Result<()> {
        for dir in [&self.schedules_dir, &self.cache_dir, &self.logs_dir] {
            if !dir.exists() {
                tokio::fs::create_dir_all(dir).await.map_err(|e| {
                    Error::Scheduler(format!(
                        "Failed to create directory '{}': {}",
                        dir.display(),
                        e
                    ))
                })?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_temp_dir() -> tempfile::TempDir {
        tempfile::tempdir().expect("Failed to create temp dir")
    }

    #[tokio::test]
    async fn test_scheduler_create_and_start() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("schedules");
        let executor = Arc::new(NoopExecutor);

        let scheduler = TaskScheduler::new(&schedules_dir, executor).await;
        assert!(scheduler.is_ok());

        let scheduler = scheduler.unwrap();
        assert_eq!(scheduler.task_count().await, 0);

        scheduler.start().await.expect("start should succeed");
        scheduler.shutdown().await.expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn test_add_task() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("schedules");
        let executor = Arc::new(NoopExecutor);

        let scheduler = TaskScheduler::new(&schedules_dir, executor).await.unwrap();
        scheduler.start().await.unwrap();

        let task = Task::builder()
            .id("test-task-1")
            .name("Test Task")
            .schedule("0 0 9 * * *")
            .steps(vec![niuma_core::Step::new(
                "step1",
                "shell",
                serde_json::json!({"command": "echo hello"}),
            )])
            .build()
            .expect("build should succeed");

        let result = scheduler.add_task(task).await;
        assert!(result.is_ok());
        assert_eq!(scheduler.task_count().await, 1);

        scheduler.shutdown().await.expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn test_remove_task() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("schedules");
        let executor = Arc::new(NoopExecutor);

        let scheduler = TaskScheduler::new(&schedules_dir, executor).await.unwrap();
        scheduler.start().await.unwrap();

        let task = Task::builder()
            .id("remove-me")
            .name("Remove Me")
            .schedule("0 0 9 * * *")
            .build()
            .expect("build should succeed");

        scheduler.add_task(task).await.expect("add should succeed");
        assert_eq!(scheduler.task_count().await, 1);

        scheduler
            .remove_task("remove-me")
            .await
            .expect("remove should succeed");
        assert_eq!(scheduler.task_count().await, 0);

        scheduler.shutdown().await.expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn test_enable_disable_task() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("schedules");
        let executor = Arc::new(NoopExecutor);

        let scheduler = TaskScheduler::new(&schedules_dir, executor).await.unwrap();
        scheduler.start().await.unwrap();

        let task = Task::builder()
            .id("toggle-me")
            .name("Toggle Me")
            .schedule("0 0 9 * * *")
            .enabled(true)
            .build()
            .expect("build should succeed");

        scheduler.add_task(task).await.expect("add should succeed");

        scheduler
            .disable_task("toggle-me")
            .await
            .expect("disable should succeed");
        let task = scheduler.get_task("toggle-me").await;
        assert!(task.is_some_and(|t| !t.enabled));

        scheduler
            .enable_task("toggle-me")
            .await
            .expect("enable should succeed");
        let task = scheduler.get_task("toggle-me").await;
        assert!(task.is_some_and(|t| t.enabled));

        scheduler.shutdown().await.expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn test_list_tasks() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("schedules");
        let executor = Arc::new(NoopExecutor);

        let scheduler = TaskScheduler::new(&schedules_dir, executor).await.unwrap();
        scheduler.start().await.unwrap();

        let task1 = Task::builder()
            .id("task-1")
            .name("Task 1")
            .schedule("0 0 9 * * *")
            .build()
            .expect("build should succeed");

        let task2 = Task::builder()
            .id("task-2")
            .name("Task 2")
            .schedule("0 0 10 * * *")
            .build()
            .expect("build should succeed");

        scheduler.add_task(task1).await.expect("add should succeed");
        scheduler.add_task(task2).await.expect("add should succeed");

        let tasks = scheduler.list_tasks().await;
        assert_eq!(tasks.len(), 2);

        scheduler.shutdown().await.expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn test_storage_config() {
        let config = StorageConfig::new("/tmp/niuma");
        assert_eq!(config.schedules_dir.to_str(), Some("/tmp/niuma/schedules"));
        assert_eq!(config.cache_dir.to_str(), Some("/tmp/niuma/cache/plans"));
        assert_eq!(config.logs_dir.to_str(), Some("/tmp/niuma/logs"));

        let temp = create_temp_dir();
        let config = StorageConfig::new(temp.path());
        let result = config.create_dirs().await;
        assert!(result.is_ok());
        assert!(config.schedules_dir.exists());
        assert!(config.cache_dir.exists());
        assert!(config.logs_dir.exists());
    }

    #[tokio::test]
    async fn test_invalid_cron_expression() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("schedules");
        let executor = Arc::new(NoopExecutor);

        let scheduler = TaskScheduler::new(&schedules_dir, executor).await.unwrap();
        scheduler.start().await.unwrap();

        let task = Task::builder()
            .id("bad-cron")
            .name("Bad Cron")
            .schedule("not a cron expression")
            .build()
            .expect("build should succeed");

        let result = scheduler.add_task(task).await;
        assert!(result.is_err());

        scheduler.shutdown().await.expect("shutdown should succeed");
    }

    #[tokio::test]
    async fn test_run_task() {
        let temp = create_temp_dir();
        let schedules_dir = temp.path().join("schedules");
        let executor = Arc::new(NoopExecutor);

        let scheduler = TaskScheduler::new(&schedules_dir, executor).await.unwrap();
        scheduler.start().await.unwrap();

        let task = Task::builder()
            .id("run-me")
            .name("Run Me")
            .schedule("0 0 9 * * *")
            .build()
            .expect("build should succeed");

        scheduler.add_task(task).await.expect("add should succeed");
        let result = scheduler.run_task("run-me").await;
        assert!(result.is_ok());

        scheduler.shutdown().await.expect("shutdown should succeed");
    }
}
