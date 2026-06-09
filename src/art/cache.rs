//! Album-art cache: an in-memory hot layer over the redb-backed [`Store`].
//!
//! The DB tracks each entry's size and last-access time so the configured
//! `art_cache_size_mb` budget is enforced via LRU eviction (see `store`).

use crate::store::Store;
use image::ImageFormat;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct ArtCache {
    store: Store,
    limit_bytes: u64,
    /// Hot layer: `Some(bytes)` = cached art, `None` = known-missing this session.
    memory: Arc<RwLock<HashMap<String, Option<Vec<u8>>>>>,
}

impl ArtCache {
    pub fn new(store: Store, limit_mb: u32) -> Self {
        Self {
            store,
            limit_bytes: limit_mb as u64 * 1024 * 1024,
            memory: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Clone-friendly handle for use in async tasks.
    pub fn clone_inner(&self) -> ArtCache {
        self.clone()
    }

    pub async fn get(&self, key: &str) -> Option<Vec<u8>> {
        {
            let mem = self.memory.read().await;
            if let Some(entry) = mem.get(key) {
                return entry.clone();
            }
        }

        let store = self.store.clone();
        let k = key.to_string();
        let data = tokio::task::spawn_blocking(move || store.art_get(&k))
            .await
            .ok()
            .flatten();

        if let Some(bytes) = &data {
            self.memory
                .write()
                .await
                .insert(key.to_string(), Some(bytes.clone()));
        }
        data
    }

    pub async fn store(
        &self,
        key: &str,
        data: &[u8],
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Downscale to a 500px JPEG to bound per-entry size.
        let processed = match image::load_from_memory(data) {
            Ok(img) => {
                let resized = img.resize(500, 500, image::imageops::FilterType::Lanczos3);
                let mut buf = Vec::new();
                let mut cursor = std::io::Cursor::new(&mut buf);
                resized.write_to(&mut cursor, ImageFormat::Jpeg)?;
                buf
            }
            Err(_) => data.to_vec(),
        };

        let store = self.store.clone();
        let k = key.to_string();
        let bytes = processed.clone();
        let limit = self.limit_bytes;
        tokio::task::spawn_blocking(move || store.art_put(&k, &bytes, limit))
            .await
            .ok();

        self.memory
            .write()
            .await
            .insert(key.to_string(), Some(processed));
        Ok(())
    }

    /// Record that this key has no art (persisted as a negative cache entry).
    pub async fn store_empty(&self, key: &str) {
        self.memory.write().await.insert(key.to_string(), None);
        let store = self.store.clone();
        let k = key.to_string();
        tokio::task::spawn_blocking(move || store.art_put_empty(&k))
            .await
            .ok();
    }

    /// Whether this key has been resolved before (positive or negative),
    /// checking the in-memory layer first, then the persisted store.
    pub async fn is_known(&self, key: &str) -> bool {
        {
            let mem = self.memory.read().await;
            if mem.contains_key(key) {
                return true;
            }
        }
        let store = self.store.clone();
        let k = key.to_string();
        tokio::task::spawn_blocking(move || store.art_known(&k))
            .await
            .unwrap_or(false)
    }

    pub async fn clear(&self) {
        self.memory.write().await.clear();
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || store.art_clear()).await.ok();
    }
}
