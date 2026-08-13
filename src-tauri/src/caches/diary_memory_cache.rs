use crate::diaries::DiaryManifest;
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DiaryMemoryCache {
    /// key: diary id, value: (manifest, etag)
    diaries: Arc<DashMap<u64, Arc<(DiaryManifest, String)>>>,
}

impl DiaryMemoryCache {
    pub fn new() -> Self {
        Self {
            diaries: Arc::new(DashMap::new()),
        }
    }

    pub fn get(&self, id: u64) -> Option<(DiaryManifest, String)> {
        self.diaries.get(&id).map(|diary| diary.as_ref().clone())
    }

    pub fn insert(&self, id: u64, diary: DiaryManifest, etag: String) {
        self.diaries.insert(id, Arc::new((diary, etag)));
    }

    pub fn remove(&self, id: u64) {
        self.diaries.remove(&id);
    }
}
