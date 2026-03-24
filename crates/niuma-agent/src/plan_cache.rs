//! Plan caching for LLM optimization.
//!
//! This module provides the [`PlanCache`] which stores distilled ExecutionPlans
//! keyed by goal hash, enabling cache hits that skip LLM planning calls.

use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
};

use niuma_core::ExecutionPlan;

use super::{Error, Result};

/// Maximum number of entries in the in-memory cache before eviction.
const DEFAULT_MAX_ENTRIES: usize = 1000;

/// Default TTL for cache entries (24 hours).
const DEFAULT_TTL_SECS: u64 = 86_400;

/// A cached execution plan entry.
#[derive(Debug, Clone)]
struct CacheEntry {
    /// The cached execution plan.
    plan: ExecutionPlan,
    /// When this entry was created.
    created_at: std::time::Instant,
    /// TTL in seconds.
    #[allow(dead_code)]
    ttl_secs: u64,
}

impl CacheEntry {
    /// Checks if the entry has expired.
    #[must_use]
    fn is_expired(&self) -> bool {
        self.created_at.elapsed().as_secs() > self.ttl_secs
    }
}

/// A plan cache that stores distilled ExecutionPlans keyed by goal hash.
///
/// The cache stores plans in memory for fast access and optionally persists
/// them to disk for durability across restarts.
#[derive(Debug)]
pub struct PlanCache {
    /// In-memory cache map.
    inner: std::sync::RwLock<HashMap<String, CacheEntry>>,
    /// Insertion order for FIFO eviction.
    order: std::sync::RwLock<VecDeque<String>>,
    /// Maximum entries before eviction.
    #[allow(dead_code)]
    max_entries: usize,
    /// Persisted cache directory (optional).
    #[allow(dead_code)]
    persist_dir: Option<PathBuf>,
}

impl PlanCache {
    /// Creates a new in-memory plan cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: std::sync::RwLock::new(HashMap::new()),
            order: std::sync::RwLock::new(VecDeque::new()),
            max_entries: DEFAULT_MAX_ENTRIES,
            persist_dir: None,
        }
    }

    /// Creates a new plan cache with a persistence directory.
    ///
    /// When a persistence directory is provided, cache entries will be
    /// persisted to disk and loaded on startup.
    ///
    /// # Errors
    ///
    /// Returns an error if the directory cannot be created.
    pub async fn with_persistence(cache_dir: impl Into<PathBuf>) -> Result<Self> {
        let cache_dir = cache_dir.into();

        if !cache_dir.exists() {
            tokio::fs::create_dir_all(&cache_dir).await.map_err(|e| {
                Error::Generic(format!(
                    "Failed to create cache directory '{}': {}",
                    cache_dir.display(),
                    e
                ))
            })?;
        }

        let mut cache = Self {
            inner: std::sync::RwLock::new(HashMap::new()),
            order: std::sync::RwLock::new(VecDeque::new()),
            max_entries: DEFAULT_MAX_ENTRIES,
            persist_dir: Some(cache_dir),
        };

        cache.load_from_disk().await?;
        Ok(cache)
    }

    /// Sets the maximum number of entries in the cache.
    #[must_use]
    pub fn with_max_entries(mut self, max: usize) -> Self {
        self.max_entries = max;
        self
    }

    /// Computes the hash for a goal string.
    #[must_use]
    pub fn hash_goal(goal: &str) -> String {
        use std::{
            collections::hash_map::DefaultHasher,
            hash::{Hash, Hasher},
        };
        let mut hasher = DefaultHasher::new();
        goal.hash(&mut hasher);
        format!("{:x}", hasher.finish())
    }

    /// Looks up a cached plan by goal hash.
    ///
    /// Returns the plan if found and not expired, `None` otherwise.
    #[must_use]
    pub fn get(&self, goal_hash: &str) -> Option<ExecutionPlan> {
        let mut inner = self.inner.write().unwrap();
        let entry = inner.get_mut(goal_hash)?;

        if entry.is_expired() {
            inner.remove(goal_hash);
            drop(inner);
            let mut order = self.order.write().unwrap();
            order.retain(|k| k != goal_hash);
            return None;
        }

        Some(entry.plan.clone())
    }

    /// Looks up a cached plan by goal string.
    ///
    /// Computes the hash and calls [`get`](Self::get).
    #[must_use]
    pub fn get_by_goal(&self, goal: &str) -> Option<ExecutionPlan> {
        let hash = Self::hash_goal(goal);
        self.get(&hash)
    }

    /// Stores a plan in the cache.
    ///
    /// If the cache is full, the oldest entry is evicted.
    pub fn put(&self, goal: &str, plan: ExecutionPlan) {
        let goal_hash = Self::hash_goal(goal);

        let mut inner = self.inner.write().unwrap();
        let mut order = self.order.write().unwrap();

        // Evict oldest entry if at capacity
        if inner.len() >= self.max_entries
            && !inner.contains_key(&goal_hash)
            && let Some(oldest) = order.pop_front()
        {
            inner.remove(&oldest);
        }

        // Remove existing entry from order if updating
        if inner.contains_key(&goal_hash) {
            order.retain(|k| k != &goal_hash);
        }

        inner.insert(
            goal_hash.clone(),
            CacheEntry {
                plan,
                created_at: std::time::Instant::now(),
                ttl_secs: DEFAULT_TTL_SECS,
            },
        );
        order.push_back(goal_hash);
    }

    /// Stores a plan with a custom TTL.
    pub fn put_with_ttl(&self, goal: &str, plan: ExecutionPlan, ttl_secs: u64) {
        let goal_hash = Self::hash_goal(goal);

        let mut inner = self.inner.write().unwrap();
        let mut order = self.order.write().unwrap();

        // Evict oldest entry if at capacity
        if inner.len() >= self.max_entries
            && !inner.contains_key(&goal_hash)
            && let Some(oldest) = order.pop_front()
        {
            inner.remove(&oldest);
        }

        // Remove existing entry from order if updating
        if inner.contains_key(&goal_hash) {
            order.retain(|k| k != &goal_hash);
        }

        inner.insert(
            goal_hash.clone(),
            CacheEntry {
                plan,
                created_at: std::time::Instant::now(),
                ttl_secs,
            },
        );
        order.push_back(goal_hash);
    }

    /// Invalidates a cache entry by goal hash.
    pub fn invalidate(&self, goal_hash: &str) {
        let mut inner = self.inner.write().unwrap();
        inner.remove(goal_hash);
        drop(inner);
        let mut order = self.order.write().unwrap();
        order.retain(|k| k != goal_hash);
    }

    /// Invalidates a cache entry by goal string.
    pub fn invalidate_by_goal(&self, goal: &str) {
        let hash = Self::hash_goal(goal);
        self.invalidate(&hash);
    }

    /// Clears all entries from the cache.
    pub fn clear(&self) {
        let mut inner = self.inner.write().unwrap();
        inner.clear();
        drop(inner);
        let mut order = self.order.write().unwrap();
        order.clear();
    }

    /// Returns the number of entries in the cache.
    #[must_use]
    pub fn len(&self) -> usize {
        let inner = self.inner.read().unwrap();
        inner.len()
    }

    /// Returns true if the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let inner = self.inner.read().unwrap();
        inner.is_empty()
    }

    /// Removes expired entries from the cache.
    pub fn prune_expired(&self) {
        let keys_to_remove: Vec<String> = {
            let inner = self.inner.read().unwrap();
            inner
                .iter()
                .filter(|(_, entry)| entry.is_expired())
                .map(|(k, _)| k.clone())
                .collect()
        };

        {
            let mut inner = self.inner.write().unwrap();
            for key in &keys_to_remove {
                inner.remove(key);
            }
        }
        let mut order = self.order.write().unwrap();
        order.retain(|k| self.inner.read().unwrap().contains_key(k));
    }

    /// Loads cache entries from disk.
    async fn load_from_disk(&mut self) -> Result<()> {
        let dir = match &self.persist_dir {
            Some(d) => d,
            None => return Ok(()),
        };

        if !dir.exists() {
            return Ok(());
        }

        let mut entries = tokio::fs::read_dir(dir)
            .await
            .map_err(|e| Error::Generic(format!("Failed to read cache directory: {}", e)))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| Error::Generic(format!("Failed to read cache entry: {}", e)))?
        {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => match serde_json::from_str::<CachedPlan>(&content) {
                        Ok(cached) => {
                            let entry = CacheEntry {
                                plan: cached.plan,
                                created_at: std::time::Instant::now(),
                                ttl_secs: cached.ttl_secs,
                            };
                            if let Ok(mut inner) = self.inner.write() {
                                inner.insert(cached.goal_hash.clone(), entry);
                            }
                            if let Ok(mut order) = self.order.write() {
                                order.push_back(cached.goal_hash);
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                path = %path.display(),
                                error = %e,
                                "Failed to parse cached plan, skipping"
                            );
                        }
                    },
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "Failed to read cache file, skipping"
                        );
                    }
                }
            }
        }

        let count = {
            if let Ok(inner) = self.inner.read() {
                inner.len()
            } else {
                0
            }
        };
        tracing::info!(
            dir = %dir.display(),
            count = count,
            "Loaded cached plans from disk"
        );

        Ok(())
    }

    /// Persists a cache entry to disk.
    #[allow(dead_code)]
    async fn persist_entry(&self, goal_hash: &str) -> Result<()> {
        let dir = match &self.persist_dir {
            Some(d) => d,
            None => return Ok(()),
        };

        let (plan, ttl) = {
            let inner = self.inner.read().unwrap();
            match inner.get(goal_hash) {
                Some(e) => (e.plan.clone(), e.ttl_secs),
                None => return Ok(()),
            }
        };

        let cached = CachedPlan {
            goal_hash: goal_hash.to_string(),
            plan,
            ttl_secs: ttl,
        };

        let content = serde_json::to_string(&cached)
            .map_err(|e| Error::Generic(format!("Failed to serialize cache entry: {}", e)))?;

        let path = dir.join(format!("{}.json", goal_hash));
        tokio::fs::write(&path, content).await.map_err(|e| {
            Error::Generic(format!(
                "Failed to write cache file '{}': {}",
                path.display(),
                e
            ))
        })?;

        Ok(())
    }
}

impl Default for PlanCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Serializable cache entry for disk persistence.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct CachedPlan {
    goal_hash: String,
    plan: ExecutionPlan,
    ttl_secs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_goal() {
        let hash1 = PlanCache::hash_goal("hello");
        let hash2 = PlanCache::hash_goal("hello");
        let hash3 = PlanCache::hash_goal("world");

        assert_eq!(hash1, hash2);
        assert_ne!(hash1, hash3);
        assert_eq!(hash1.len(), 16); // 64-bit hash as hex = 16 chars
    }

    #[test]
    fn test_plan_cache_basic() {
        let cache = PlanCache::new();

        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);

        let plan = ExecutionPlan::new(vec![niuma_core::Step::new(
            "1",
            "shell",
            serde_json::json!({}),
        )]);

        cache.put("my goal", plan.clone());

        assert!(!cache.is_empty());
        assert_eq!(cache.len(), 1);

        let cached = cache.get_by_goal("my goal");
        assert!(cached.is_some());
        assert_eq!(cached.unwrap().steps.len(), 1);
    }

    #[test]
    fn test_plan_cache_miss() {
        let cache = PlanCache::new();
        let result = cache.get_by_goal("nonexistent");
        assert!(result.is_none());
    }

    #[test]
    fn test_plan_cache_invalidate() {
        let cache = PlanCache::new();
        let plan = ExecutionPlan::new(vec![]);
        cache.put("test", plan);

        assert!(cache.get_by_goal("test").is_some());

        cache.invalidate_by_goal("test");
        assert!(cache.get_by_goal("test").is_none());
    }

    #[test]
    fn test_plan_cache_clear() {
        let cache = PlanCache::new();
        let plan = ExecutionPlan::new(vec![]);
        cache.put("a", plan.clone());
        cache.put("b", plan.clone());
        cache.put("c", plan);

        assert_eq!(cache.len(), 3);

        cache.clear();
        assert!(cache.is_empty());
    }

    #[test]
    fn test_plan_cache_eviction() {
        let cache = PlanCache::new().with_max_entries(2);

        let plan = ExecutionPlan::new(vec![]);
        cache.put("a", plan.clone());
        cache.put("b", plan.clone());
        cache.put("c", plan);

        assert_eq!(cache.len(), 2);
        // Oldest entry "a" should be evicted
        assert!(cache.get_by_goal("a").is_none());
        assert!(cache.get_by_goal("b").is_some());
        assert!(cache.get_by_goal("c").is_some());
    }

    #[test]
    fn test_plan_cache_update_existing() {
        let cache = PlanCache::new().with_max_entries(2);

        let plan1 = ExecutionPlan::new(vec![niuma_core::Step::new(
            "1",
            "shell",
            serde_json::json!({"step": 1}),
        )]);
        let plan2 = ExecutionPlan::new(vec![niuma_core::Step::new(
            "2",
            "shell",
            serde_json::json!({"step": 2}),
        )]);

        cache.put("a", plan1);
        cache.put("b", plan2.clone());

        // Updating "a" should not evict "b"
        cache.put("a", plan2);

        assert_eq!(cache.len(), 2);
        assert!(cache.get_by_goal("a").is_some());
        assert!(cache.get_by_goal("b").is_some());
    }
}
