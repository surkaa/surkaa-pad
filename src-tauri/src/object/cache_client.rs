use crate::object::OssClient;
use crate::utils::LocalCache;

#[derive(Clone)]
pub struct CacheOssClient {
    oss: OssClient,
    cache: LocalCache,
}

impl CacheOssClient {
    pub fn new(oss: OssClient, cache: LocalCache) -> Self {
        Self {
            oss,
            cache,
        }
    }
}
