use super::*;
use crate::vault_key_change::cloud_claim::CloudClaim;

#[tokio::test]
async fn rekey_commands_cannot_touch_data_while_another_instance_holds_the_process_lock() {
    let temp = tempfile::tempdir().unwrap();
    let state = persistent_state(temp.path());
    let (_, bootstrap) = crypto("old");
    state.persist_vault_bootstrap(bootstrap).unwrap();
    let another = persistent_state(temp.path());
    let guard = another.lock_key_change_process().unwrap().unwrap();
    let before = std::fs::read(temp.path().join("app-state.json")).unwrap();
    let busy =
        |failure: crate::error::AppError| assert!(failure.to_string().contains("本机另一应用实例"));
    use crate::vault_key_change::command;
    busy(
        command::plan_change(
            &state,
            "old".into(),
            "new".into(),
            "new".into(),
            None,
            Default::default(),
        )
        .await
        .err()
        .unwrap(),
    );
    busy(
        command::start_change(&state, "no-plan".into(), channel())
            .await
            .err()
            .unwrap(),
    );
    busy(
        command::resume_change(&state, "new".into(), channel())
            .await
            .err()
            .unwrap(),
    );
    busy(
        command::sync_changed_key(&state, "new".into(), Default::default(), channel())
            .await
            .err()
            .unwrap(),
    );
    busy(
        command::ack_change(&state, Default::default())
            .await
            .err()
            .unwrap(),
    );
    assert_eq!(
        std::fs::read(temp.path().join("app-state.json")).unwrap(),
        before
    );
    assert!(Storage {
        state: state.clone(),
        remote: false
    }
    .list()
    .await
    .unwrap()
    .is_empty());
    assert!(state.pending_key_change().is_none());
    drop(guard);
    command::ack_change(&state, Default::default())
        .await
        .unwrap();
}

#[tokio::test]
async fn withdraws_only_an_unstarted_owned_claim_and_keeps_all_recovery_on_uncertainty() {
    for case in [
        "fresh",
        "progress",
        "staged",
        "committed",
        "orphan",
        "foreign",
        "delete-error",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let state = persistent_state(temp.path());
        let store = MemoryStore::default();
        let (old, ob) = crypto("old");
        let (new, nb) = crypto("new");
        fixture(&store, &old, 17).await;
        let (_, items) = inventory(&store, &old).await.unwrap();
        let (mut journal, mut checkpoint) =
            Journal::create(&old, &new, ob, nb, true, items, Default::default()).unwrap();
        match case {
            "progress" => checkpoint.next = 1,
            "staged" => {
                checkpoint.staged = Some(VerifiedStage {
                    size: 1,
                    old_cipher_hash: "old".into(),
                    new_cipher_hash: "new".into(),
                    cloud: None,
                })
            }
            "committed" => {
                checkpoint.next = checkpoint.items.len();
                checkpoint.data_committed = true;
            }
            "orphan" => {
                store.0.lock().unwrap().objects.insert(
                    ObjectLocations::vault_key_change_stage(&journal.transaction_id, 0),
                    b"recovery".to_vec(),
                );
            }
            _ => (),
        }
        journal.update(&new, &checkpoint).unwrap();
        let encoded = journal.encode().unwrap();
        state.persist_key_change(Some(encoded.clone())).unwrap();
        let claim =
            CloudClaim::new(&journal.transaction_id, &journal.old_bootstrap.vault_id).unwrap();
        claim.publish(&store).await.unwrap();
        let claim_key = ObjectLocations::vault_key_change_claim(&journal.transaction_id);
        let competitor_key = ObjectLocations::vault_key_change_claim(&"a".repeat(32));
        {
            let mut data = store.0.lock().unwrap();
            data.objects
                .insert(competitor_key.clone(), b"foreign-claim".to_vec());
            data.objects.insert(
                ObjectLocations::vault_key_change().into(),
                b"foreign-journal".to_vec(),
            );
            if case == "foreign" {
                data.objects
                    .insert(claim_key.clone(), b"corrupted-or-foreign".to_vec());
            } else if case == "delete-error" {
                // 枚举 -> HEAD -> GET -> DELETE；删除失败不能丢弃本机恢复记录。
                data.fail_at = Some(data.calls + 4);
            }
        }
        let before = store.0.lock().unwrap().objects.clone();
        let result = crate::vault_key_change::command::withdraw_unstarted_claim(
            &state,
            &store,
            &journal,
            &checkpoint,
            &claim,
        )
        .await;
        if case == "fresh" {
            assert!(result.unwrap());
            assert!(state.pending_key_change().is_none());
            let mut expected = before;
            expected.remove(&claim_key);
            assert_eq!(store.0.lock().unwrap().objects, expected);
            assert!(!crate::vault_key_change::command::withdraw_unstarted_claim(
                &state,
                &store,
                &journal,
                &Checkpoint {
                    next: 1,
                    ..checkpoint
                },
                &claim
            )
            .await
            .unwrap());
        } else {
            if matches!(case, "foreign" | "delete-error") {
                assert!(result.is_err(), "{case}");
            } else {
                assert!(!result.unwrap(), "{case}");
            }
            assert_eq!(
                state.pending_key_change().as_deref(),
                Some(encoded.as_str()),
                "{case}"
            );
            assert_eq!(store.0.lock().unwrap().objects, before, "{case}");
        }
    }
}
