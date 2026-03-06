use image::ImageFormat;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct ArtCache {
    cache_dir: PathBuf,
    memory: Arc<RwLock<HashMap<String, Option<Vec<u8>>>>>,
}

impl ArtCache {
    pub fn new(cache_dir: PathBuf) -> Self {
        std::fs::create_dir_all(&cache_dir).ok();
        Self {
            cache_dir,
            memory: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Clone-friendly handle for use in async tasks
    pub fn clone_inner(&self) -> ArtCache {
        ArtCache {
            cache_dir: self.cache_dir.clone(),
            memory: Arc::clone(&self.memory),
        }
    }

    fn filename(&self, key: &str) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        let hash = hasher.finish();
        self.cache_dir.join(format!("{hash:016x}.jpg"))
    }

    pub async fn get(&self, key: &str) -> Option<Vec<u8>> {
        {
            let mem = self.memory.read().await;
            if let Some(entry) = mem.get(key) {
                return entry.clone();
            }
        }

        let path = self.filename(key);
        if path.exists() {
            if let Ok(data) = tokio::fs::read(&path).await {
                let mut mem = self.memory.write().await;
                mem.insert(key.to_string(), Some(data.clone()));
                return Some(data);
            }
        }

        None
    }

    pub async fn store(&self, key: &str, data: &[u8]) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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

        let path = self.filename(key);
        tokio::fs::write(&path, &processed).await?;

        let mut mem = self.memory.write().await;
        mem.insert(key.to_string(), Some(processed));

        Ok(())
    }

    pub async fn store_empty(&self, key: &str) {
        let mut mem = self.memory.write().await;
        mem.insert(key.to_string(), None);
    }

    pub async fn is_known(&self, key: &str) -> bool {
        let mem = self.memory.read().await;
        mem.contains_key(key)
    }

    pub async fn clear(&self) {
        let mut mem = self.memory.write().await;
        mem.clear();
        if let Ok(entries) = std::fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                std::fs::remove_file(entry.path()).ok();
            }
        }
    }
}
