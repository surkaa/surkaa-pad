use crate::secure_diary_store::DiaryManifest;
use dashmap::DashMap;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct DiaryCache {
    diaries: Arc<Mutex<DashMap<String, Arc<DiaryManifest>>>>,
}

impl DiaryCache {
    pub fn new() -> Self {
        Self {
            diaries: Arc::new(Mutex::new(DashMap::new())),
        }
    }

    pub fn get(&self, id: &str) -> Option<DiaryManifest> {
        let diaries = self.diaries.lock().unwrap();
        diaries.get(id).map(|pad| pad.as_ref().clone())
    }

    pub fn insert(&self, id: &str, pad: DiaryManifest) {
        let diaries = self.diaries.lock().unwrap();
        diaries.insert(id.to_string(), Arc::new(pad));
    }

    pub fn clean(&self) {
        let diaries = self.diaries.lock().unwrap();
        diaries.clear();
    }

    pub fn list(&self) -> Vec<DiaryManifest> {
        let diaries = self.diaries.lock().unwrap();
        diaries.iter().map(|entry| entry.as_ref().clone()).collect()
    }
}
