use super::session_types::validate_session_message_block;
use super::{AiSessionMessage, AiSessionMessageBlock, CURRENT_AI_SESSION_VERSION};
use crate::object_locations::MAX_AI_MESSAGE_BLOCK_LEVEL;
use async_trait::async_trait;
use std::collections::BTreeMap;
use thiserror::Error;

const BLOCK_RADIX: u64 = 10;

#[derive(Debug, Error)]
pub enum AiMessageBlockError {
    #[error("AI 消息块存储失败: {0}")]
    Storage(String),
    #[error("AI 消息块无效: {0}")]
    InvalidBlock(String),
    #[error("缺少待合并的 {level} 级消息块 {block_id}")]
    MissingSourceBlock { level: u32, block_id: u64 },
    #[error("消息块 {level}/{block_id} 与已写入的高等级块内容冲突")]
    ConflictingBlock { level: u32, block_id: u64 },
    #[error("AI 消息块索引计算溢出")]
    IndexOverflow,
    #[error("AI 消息索引不连续：期望 {expected}，实际为 {actual}")]
    NonContiguousMessages { expected: u64, actual: u64 },
}

#[async_trait]
pub trait AiMessageBlockStore: Send + Sync {
    async fn load_block(
        &self,
        session_id: &str,
        level: u32,
        block_id: u64,
    ) -> Result<Option<AiSessionMessageBlock>, AiMessageBlockError>;

    async fn save_block(&self, block: &AiSessionMessageBlock) -> Result<(), AiMessageBlockError>;

    async fn delete_block(
        &self,
        session_id: &str,
        level: u32,
        block_id: u64,
    ) -> Result<(), AiMessageBlockError>;

    async fn list_blocks(
        &self,
        session_id: &str,
    ) -> Result<Vec<AiSessionMessageBlock>, AiMessageBlockError>;
}

/// 先把消息作为 0 级块写入，再按十进制进位规则逐级合并。
///
/// 每一级都严格遵循“先写目标块，再删来源块”。如果删除中断，重试时会校验已存在
/// 的目标块，并继续清理残留来源块，因此不会因合并失败丢失消息。
pub async fn append_and_compact_message(
    store: &dyn AiMessageBlockStore,
    session_id: &str,
    message: AiSessionMessage,
) -> Result<(), AiMessageBlockError> {
    let message_index = message.index;
    let block = AiSessionMessageBlock {
        version: CURRENT_AI_SESSION_VERSION,
        session_id: session_id.to_owned(),
        level: 0,
        block_id: message_index,
        messages: vec![message],
    };
    validate_block(&block)?;
    store.save_block(&block).await?;

    let mut level = 0;
    let mut block_id = message_index;
    while level < MAX_AI_MESSAGE_BLOCK_LEVEL && block_id % BLOCK_RADIX == BLOCK_RADIX - 1 {
        let target = compact_completed_group(store, session_id, level, block_id).await?;
        level = target.level;
        block_id = target.block_id;
    }
    Ok(())
}

/// 加载并按全局索引恢复消息。高等级块和未清理的低等级块重叠时只保留一份；若内容
/// 不一致则直接报错，避免静默选择潜在损坏的数据。
pub async fn load_compacted_messages(
    store: &dyn AiMessageBlockStore,
    session_id: &str,
    expected_count: u64,
) -> Result<Vec<AiSessionMessage>, AiMessageBlockError> {
    let messages = load_all_compacted_messages(store, session_id).await?;
    let actual_count =
        u64::try_from(messages.len()).map_err(|_| AiMessageBlockError::IndexOverflow)?;
    if actual_count != expected_count {
        return Err(AiMessageBlockError::NonContiguousMessages {
            expected: expected_count,
            actual: actual_count,
        });
    }
    Ok(messages)
}

/// 不依赖 meta 中的提交水位，从当前所有消息块恢复完整且连续的物理消息序列。
///
/// 该入口用于会话首次打开或上次提交失败后的协调。正常追加仍使用 meta 中的水位，
/// 不会在每次写入前重复扫描全部消息块。
pub async fn load_all_compacted_messages(
    store: &dyn AiMessageBlockStore,
    session_id: &str,
) -> Result<Vec<AiSessionMessage>, AiMessageBlockError> {
    let mut messages = BTreeMap::<u64, AiSessionMessage>::new();
    for block in store.list_blocks(session_id).await? {
        ensure_block_location(&block, session_id, block.level, block.block_id)?;
        validate_block(&block)?;
        for message in block.messages {
            match messages.get(&message.index) {
                Some(existing) if existing != &message => {
                    return Err(AiMessageBlockError::ConflictingBlock {
                        level: block.level,
                        block_id: block.block_id,
                    });
                }
                Some(_) => {}
                None => {
                    messages.insert(message.index, message);
                }
            }
        }
    }

    for (expected, actual) in (0_u64..).zip(messages.keys().copied()) {
        if expected != actual {
            return Err(AiMessageBlockError::NonContiguousMessages { expected, actual });
        }
    }
    Ok(messages.into_values().collect())
}

async fn compact_completed_group(
    store: &dyn AiMessageBlockStore,
    session_id: &str,
    source_level: u32,
    last_source_id: u64,
) -> Result<AiSessionMessageBlock, AiMessageBlockError> {
    let first_source_id = last_source_id
        .checked_sub(BLOCK_RADIX - 1)
        .ok_or(AiMessageBlockError::IndexOverflow)?;
    let target_level = source_level
        .checked_add(1)
        .ok_or(AiMessageBlockError::IndexOverflow)?;
    let target_id = last_source_id / BLOCK_RADIX;

    let existing_target = store
        .load_block(session_id, target_level, target_id)
        .await?;
    let target = if let Some(target) = existing_target {
        ensure_block_location(&target, session_id, target_level, target_id)?;
        validate_block(&target)?;
        validate_remaining_sources_against_target(
            store,
            session_id,
            source_level,
            first_source_id,
            &target,
        )
        .await?;
        target
    } else {
        let sources =
            load_complete_source_group(store, session_id, source_level, first_source_id).await?;
        let target = merge_source_blocks(session_id, target_level, target_id, sources)?;
        // 这是合并的提交点；在它成功前绝不能删除任何来源块。
        store.save_block(&target).await?;
        target
    };

    for source_id in first_source_id..=last_source_id {
        store
            .delete_block(session_id, source_level, source_id)
            .await?;
    }
    Ok(target)
}

async fn load_complete_source_group(
    store: &dyn AiMessageBlockStore,
    session_id: &str,
    level: u32,
    first_block_id: u64,
) -> Result<Vec<AiSessionMessageBlock>, AiMessageBlockError> {
    let mut blocks = Vec::with_capacity(BLOCK_RADIX as usize);
    for offset in 0..BLOCK_RADIX {
        let block_id = first_block_id
            .checked_add(offset)
            .ok_or(AiMessageBlockError::IndexOverflow)?;
        let block = store
            .load_block(session_id, level, block_id)
            .await?
            .ok_or(AiMessageBlockError::MissingSourceBlock { level, block_id })?;
        ensure_block_location(&block, session_id, level, block_id)?;
        validate_block(&block)?;
        blocks.push(block);
    }
    Ok(blocks)
}

async fn validate_remaining_sources_against_target(
    store: &dyn AiMessageBlockStore,
    session_id: &str,
    source_level: u32,
    first_source_id: u64,
    target: &AiSessionMessageBlock,
) -> Result<(), AiMessageBlockError> {
    for offset in 0..BLOCK_RADIX {
        let source_id = first_source_id
            .checked_add(offset)
            .ok_or(AiMessageBlockError::IndexOverflow)?;
        let Some(source) = store
            .load_block(session_id, source_level, source_id)
            .await?
        else {
            continue;
        };
        ensure_block_location(&source, session_id, source_level, source_id)?;
        validate_block(&source)?;

        let source_len = source.messages.len();
        let start = usize::try_from(offset)
            .ok()
            .and_then(|offset| offset.checked_mul(source_len))
            .ok_or(AiMessageBlockError::IndexOverflow)?;
        let end = start
            .checked_add(source_len)
            .ok_or(AiMessageBlockError::IndexOverflow)?;
        if target.messages.get(start..end) != Some(source.messages.as_slice()) {
            return Err(AiMessageBlockError::ConflictingBlock {
                level: source_level,
                block_id: source_id,
            });
        }
    }
    Ok(())
}

fn merge_source_blocks(
    session_id: &str,
    target_level: u32,
    target_id: u64,
    sources: Vec<AiSessionMessageBlock>,
) -> Result<AiSessionMessageBlock, AiMessageBlockError> {
    let messages = sources
        .into_iter()
        .flat_map(|block| block.messages)
        .collect();
    let target = AiSessionMessageBlock {
        version: CURRENT_AI_SESSION_VERSION,
        session_id: session_id.to_owned(),
        level: target_level,
        block_id: target_id,
        messages,
    };
    validate_block(&target)?;
    Ok(target)
}

fn ensure_block_location(
    block: &AiSessionMessageBlock,
    session_id: &str,
    level: u32,
    block_id: u64,
) -> Result<(), AiMessageBlockError> {
    if block.session_id != session_id || block.level != level || block.block_id != block_id {
        return Err(AiMessageBlockError::InvalidBlock(format!(
            "消息块实际位置为 {}/{}/{}，期望为 {session_id}/{level}/{block_id}",
            block.session_id, block.level, block.block_id
        )));
    }
    Ok(())
}

fn validate_block(block: &AiSessionMessageBlock) -> Result<(), AiMessageBlockError> {
    validate_session_message_block(block)
        .map_err(|error| AiMessageBlockError::InvalidBlock(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{AiSessionMessagePayload, CURRENT_AI_SESSION_VERSION};
    use std::collections::{BTreeMap, HashSet};
    use tokio::sync::Mutex;

    type BlockKey = (String, u32, u64);

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum StoreEvent {
        Save(u32, u64),
        Delete(u32, u64),
    }

    #[derive(Default)]
    struct MemoryBlockStore {
        blocks: Mutex<BTreeMap<BlockKey, AiSessionMessageBlock>>,
        events: Mutex<Vec<StoreEvent>>,
        fail_saves: Mutex<HashSet<(u32, u64)>>,
        fail_deletes_once: Mutex<HashSet<(u32, u64)>>,
    }

    impl MemoryBlockStore {
        async fn layout(&self, session_id: &str) -> Vec<(u32, u64)> {
            self.blocks
                .lock()
                .await
                .keys()
                .filter(|(id, _, _)| id == session_id)
                .map(|(_, level, block_id)| (*level, *block_id))
                .collect()
        }

        async fn remove(&self, session_id: &str, level: u32, block_id: u64) {
            self.blocks
                .lock()
                .await
                .remove(&(session_id.to_owned(), level, block_id));
        }

        async fn insert(&self, block: AiSessionMessageBlock) {
            self.blocks.lock().await.insert(
                (block.session_id.clone(), block.level, block.block_id),
                block,
            );
        }

        async fn clear_events(&self) {
            self.events.lock().await.clear();
        }
    }

    #[async_trait]
    impl AiMessageBlockStore for MemoryBlockStore {
        async fn load_block(
            &self,
            session_id: &str,
            level: u32,
            block_id: u64,
        ) -> Result<Option<AiSessionMessageBlock>, AiMessageBlockError> {
            Ok(self
                .blocks
                .lock()
                .await
                .get(&(session_id.to_owned(), level, block_id))
                .cloned())
        }

        async fn save_block(
            &self,
            block: &AiSessionMessageBlock,
        ) -> Result<(), AiMessageBlockError> {
            self.events
                .lock()
                .await
                .push(StoreEvent::Save(block.level, block.block_id));
            if self
                .fail_saves
                .lock()
                .await
                .contains(&(block.level, block.block_id))
            {
                return Err(AiMessageBlockError::Storage("模拟写入失败".into()));
            }
            self.insert(block.clone()).await;
            Ok(())
        }

        async fn delete_block(
            &self,
            session_id: &str,
            level: u32,
            block_id: u64,
        ) -> Result<(), AiMessageBlockError> {
            self.events
                .lock()
                .await
                .push(StoreEvent::Delete(level, block_id));
            if self
                .fail_deletes_once
                .lock()
                .await
                .remove(&(level, block_id))
            {
                return Err(AiMessageBlockError::Storage("模拟删除失败".into()));
            }
            self.remove(session_id, level, block_id).await;
            Ok(())
        }

        async fn list_blocks(
            &self,
            session_id: &str,
        ) -> Result<Vec<AiSessionMessageBlock>, AiMessageBlockError> {
            Ok(self
                .blocks
                .lock()
                .await
                .iter()
                .filter(|((id, _, _), _)| id == session_id)
                .map(|(_, block)| block.clone())
                .collect())
        }
    }

    fn message(index: u64) -> AiSessionMessage {
        AiSessionMessage {
            index,
            created_at: i64::try_from(index).unwrap(),
            payload: AiSessionMessagePayload::User {
                content: format!("消息 {index}"),
                timezone_offset_minutes: None,
            },
        }
    }

    async fn append_range(store: &MemoryBlockStore, count: u64) {
        for index in 0..count {
            append_and_compact_message(store, "1", message(index))
                .await
                .unwrap();
        }
    }

    #[tokio::test]
    async fn keeps_only_the_last_fragment_at_level_zero() {
        let store = MemoryBlockStore::default();
        append_range(&store, 25).await;

        assert_eq!(
            store.layout("1").await,
            vec![(0, 20), (0, 21), (0, 22), (0, 23), (0, 24), (1, 0), (1, 1)]
        );
        let messages = load_compacted_messages(&store, "1", 25).await.unwrap();
        assert_eq!(
            messages
                .iter()
                .map(|message| message.index)
                .collect::<Vec<_>>(),
            (0..25).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn carries_compaction_across_multiple_levels() {
        let cases = [
            (9, (0..9).map(|id| (0, id)).collect::<Vec<_>>()),
            (10, vec![(1, 0)]),
            (100, vec![(2, 0)]),
            (
                105,
                vec![(0, 100), (0, 101), (0, 102), (0, 103), (0, 104), (2, 0)],
            ),
            (
                250,
                vec![(1, 20), (1, 21), (1, 22), (1, 23), (1, 24), (2, 0), (2, 1)],
            ),
        ];

        for (count, expected) in cases {
            let store = MemoryBlockStore::default();
            append_range(&store, count).await;
            assert_eq!(store.layout("1").await, expected, "count={count}");
            assert_eq!(
                load_compacted_messages(&store, "1", count)
                    .await
                    .unwrap()
                    .len(),
                count as usize
            );
        }
    }

    #[tokio::test]
    async fn decimal_carry_at_one_thousand_replaces_all_lower_levels() {
        let store = MemoryBlockStore::default();
        append_range(&store, 999).await;

        let mut expected = Vec::new();
        expected.extend((990..999).map(|id| (0, id)));
        expected.extend((90..99).map(|id| (1, id)));
        expected.extend((0..9).map(|id| (2, id)));
        expected.sort_unstable();
        assert_eq!(store.layout("1").await, expected);
        assert_eq!(
            load_compacted_messages(&store, "1", 999)
                .await
                .unwrap()
                .len(),
            999
        );

        append_and_compact_message(&store, "1", message(999))
            .await
            .unwrap();
        assert_eq!(store.layout("1").await, vec![(3, 0)]);
        let messages = load_compacted_messages(&store, "1", 1_000).await.unwrap();
        assert_eq!(messages.first().map(|message| message.index), Some(0));
        assert_eq!(messages.last().map(|message| message.index), Some(999));
    }

    #[tokio::test]
    async fn appends_and_restores_more_than_one_hundred_thousand_messages() {
        const MESSAGE_COUNT: u64 = 123_456;
        let store = MemoryBlockStore::default();
        append_range(&store, MESSAGE_COUNT).await;

        assert_eq!(
            store.layout("1").await,
            expected_decimal_layout(MESSAGE_COUNT)
        );

        let messages = load_compacted_messages(&store, "1", MESSAGE_COUNT)
            .await
            .unwrap();
        assert_eq!(messages.len(), usize::try_from(MESSAGE_COUNT).unwrap());
        for (expected_index, message) in (0..MESSAGE_COUNT).zip(messages) {
            assert_eq!(message.index, expected_index);
            assert_eq!(
                message.payload,
                AiSessionMessagePayload::User {
                    content: format!("消息 {expected_index}"),
                    timezone_offset_minutes: None,
                }
            );
        }
    }

    fn expected_decimal_layout(message_count: u64) -> Vec<(u32, u64)> {
        let mut layout = Vec::new();
        let mut level = 0;
        let mut block_size = 1;
        while block_size <= message_count {
            let block_count = (message_count / block_size) % BLOCK_RADIX;
            let first_block_id = message_count / block_size / BLOCK_RADIX * BLOCK_RADIX;
            layout.extend((0..block_count).map(|offset| (level, first_block_id + offset)));
            let Some(next_size) = block_size.checked_mul(BLOCK_RADIX) else {
                break;
            };
            block_size = next_size;
            level += 1;
        }
        layout.sort_unstable();
        layout
    }

    #[tokio::test]
    async fn writes_target_before_deleting_any_source_block() {
        let store = MemoryBlockStore::default();
        append_range(&store, 9).await;
        store.clear_events().await;

        append_and_compact_message(&store, "1", message(9))
            .await
            .unwrap();
        let events = store.events.lock().await.clone();
        assert_eq!(events[0], StoreEvent::Save(0, 9));
        assert_eq!(events[1], StoreEvent::Save(1, 0));
        assert_eq!(events[2], StoreEvent::Delete(0, 0));
        assert_eq!(events.last(), Some(&StoreEvent::Delete(0, 9)));
    }

    #[tokio::test]
    async fn target_write_failure_preserves_every_source_block() {
        let store = MemoryBlockStore::default();
        append_range(&store, 9).await;
        store.fail_saves.lock().await.insert((1, 0));

        assert!(matches!(
            append_and_compact_message(&store, "1", message(9)).await,
            Err(AiMessageBlockError::Storage(_))
        ));
        assert_eq!(
            store.layout("1").await,
            (0..10).map(|id| (0, id)).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn interrupted_source_cleanup_is_idempotently_resumed() {
        let store = MemoryBlockStore::default();
        append_range(&store, 9).await;
        store.fail_deletes_once.lock().await.insert((0, 4));

        assert!(matches!(
            append_and_compact_message(&store, "1", message(9)).await,
            Err(AiMessageBlockError::Storage(_))
        ));
        assert_eq!(
            store.layout("1").await,
            vec![(0, 4), (0, 5), (0, 6), (0, 7), (0, 8), (0, 9), (1, 0)]
        );
        assert_eq!(
            load_compacted_messages(&store, "1", 10)
                .await
                .unwrap()
                .len(),
            10
        );

        append_and_compact_message(&store, "1", message(9))
            .await
            .unwrap();
        assert_eq!(store.layout("1").await, vec![(1, 0)]);
    }

    #[tokio::test]
    async fn interrupted_cleanup_during_cascading_compaction_resumes_every_level() {
        let store = MemoryBlockStore::default();
        append_range(&store, 99).await;
        store.fail_deletes_once.lock().await.insert((1, 4));

        assert!(matches!(
            append_and_compact_message(&store, "1", message(99)).await,
            Err(AiMessageBlockError::Storage(_))
        ));
        assert_eq!(
            store.layout("1").await,
            vec![(1, 4), (1, 5), (1, 6), (1, 7), (1, 8), (1, 9), (2, 0)]
        );
        assert_eq!(
            load_compacted_messages(&store, "1", 100)
                .await
                .unwrap()
                .len(),
            100
        );

        append_and_compact_message(&store, "1", message(99))
            .await
            .unwrap();
        assert_eq!(store.layout("1").await, vec![(2, 0)]);
    }

    #[tokio::test]
    async fn refuses_to_delete_sources_when_existing_target_conflicts() {
        let store = MemoryBlockStore::default();
        append_range(&store, 9).await;
        let mut conflicting_messages: Vec<_> = (0..10).map(message).collect();
        conflicting_messages[0].payload = AiSessionMessagePayload::User {
            content: "不同内容".into(),
            timezone_offset_minutes: None,
        };
        store
            .insert(AiSessionMessageBlock {
                version: CURRENT_AI_SESSION_VERSION,
                session_id: "1".into(),
                level: 1,
                block_id: 0,
                messages: conflicting_messages,
            })
            .await;

        assert!(matches!(
            append_and_compact_message(&store, "1", message(9)).await,
            Err(AiMessageBlockError::ConflictingBlock {
                level: 0,
                block_id: 0,
            })
        ));
        assert!(store.layout("1").await.contains(&(0, 0)));
    }

    #[tokio::test]
    async fn missing_source_never_creates_an_incomplete_target() {
        let store = MemoryBlockStore::default();
        append_range(&store, 9).await;
        store.remove("1", 0, 4).await;

        assert!(matches!(
            append_and_compact_message(&store, "1", message(9)).await,
            Err(AiMessageBlockError::MissingSourceBlock {
                level: 0,
                block_id: 4,
            })
        ));
        assert!(!store.layout("1").await.contains(&(1, 0)));
    }

    #[tokio::test]
    async fn loader_rejects_gaps_extra_messages_and_conflicting_overlap() {
        let store = MemoryBlockStore::default();
        append_range(&store, 10).await;
        assert!(matches!(
            load_compacted_messages(&store, "1", 9).await,
            Err(AiMessageBlockError::NonContiguousMessages {
                expected: 9,
                actual: 10,
            })
        ));

        let overlapping = AiSessionMessageBlock {
            version: CURRENT_AI_SESSION_VERSION,
            session_id: "1".into(),
            level: 0,
            block_id: 0,
            messages: vec![AiSessionMessage {
                payload: AiSessionMessagePayload::User {
                    content: "冲突".into(),
                    timezone_offset_minutes: None,
                },
                ..message(0)
            }],
        };
        store.insert(overlapping).await;
        assert!(matches!(
            load_compacted_messages(&store, "1", 10).await,
            Err(AiMessageBlockError::ConflictingBlock {
                level: 1,
                block_id: 0,
            } | AiMessageBlockError::ConflictingBlock {
                level: 0,
                block_id: 0,
            })
        ));
    }
}
