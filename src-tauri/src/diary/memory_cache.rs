use crate::diary::types::DiaryManifest;
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DiaryMemoryCache {
    /// key: diary id, value: (manifest, etag)
    diaries: Arc<DashMap<String, Arc<(DiaryManifest, String)>>>,
}

impl DiaryMemoryCache {
    pub fn new() -> Self {
        Self {
            diaries: Arc::new(DashMap::new()),
        }
    }

    pub fn get(&self, id: &str) -> Option<(DiaryManifest, String)> {
        match self.diaries.get(id) {
            Some(diary) => Some(diary.as_ref().clone()),
            None => None,
        }
    }

    pub fn insert(&self, id: &str, diary: DiaryManifest, etag: String) {
        self.diaries.insert(id.to_string(), Arc::new((diary, etag)));
    }

    pub fn remove(&self, id: &str) {
        self.diaries.remove(id);
    }
}
