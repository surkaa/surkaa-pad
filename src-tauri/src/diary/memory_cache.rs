use crate::diary::types::DiaryManifest;
use dashmap::DashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct DiaryMemoryCache {
    diaries: Arc<DashMap<String, Arc<DiaryManifest>>>,
}

impl DiaryMemoryCache {
    pub fn new() -> Self {
        Self {
            diaries: Arc::new(DashMap::new()),
        }
    }

    pub fn get(&self, id: &str) -> Option<DiaryManifest> {
        match self.diaries.get(id) {
            Some(diary) => Some(diary.as_ref().clone()),
            None => None,
        }
    }

    pub fn insert(&self, id: &str, pad: DiaryManifest) {
        self.diaries.insert(id.to_string(), Arc::new(pad));
    }

    pub fn list(&self) -> Vec<DiaryManifest> {
        self.diaries.iter().map(|v| v.as_ref().clone()).collect()
    }
}
