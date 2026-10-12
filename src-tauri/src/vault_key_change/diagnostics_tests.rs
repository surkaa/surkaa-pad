use super::*;
use crate::stream::collect_data;
use bytes::Bytes;

fn diagnostics() -> Diagnostics {
    Diagnostics::new("anonymous-transaction".into(), false, 2, 1024)
}

#[tokio::test]
async fn stream_observation_preserves_bytes_errors_and_eof_without_extra_reads() {
    let stats = diagnostics();
    let stream: ByteStream = Box::pin(futures_util::stream::iter([
        Ok(Bytes::from_static(b"a")),
        Ok(Bytes::new()),
        Ok(Bytes::from_static(b"bc")),
    ]));
    assert_eq!(
        collect_data(stats.read_stream(stream)).await.unwrap(),
        b"abc"
    );
    assert_eq!(stats.snapshot().read_bytes, 3);
    let report = stats.snapshot();
    assert_eq!(
        (report.operations[0].count, report.operations[0].failed),
        (1, 0)
    );

    let stream: ByteStream = Box::pin(futures_util::stream::iter([
        Ok(Bytes::from_static(b"d")),
        Err(std::io::Error::other("test-only-secret")),
    ]));
    assert!(collect_data(stats.read_stream(stream)).await.is_err());
    let report = stats.snapshot();
    assert_eq!(report.read_bytes, 4);
    assert_eq!(
        (report.operations[0].count, report.operations[0].failed),
        (2, 1)
    );
    assert!(!serde_json::to_string(&report)
        .unwrap()
        .contains("test-only-secret"));
}

#[tokio::test]
async fn dropped_stream_is_not_reported_as_a_success_and_supplied_write_bytes_are_exact() {
    let stats = diagnostics();
    let stream = super::super::storage::byte_stream(b"partial".to_vec());
    let mut stream = stats.read_stream(stream);
    assert!(stream.next().await.unwrap().is_ok());
    drop(stream);
    assert_eq!(stats.snapshot().operations[0].failed, 1);
    assert_eq!(
        collect_data(stats.write_stream(super::super::storage::byte_stream(vec![7; 11])))
            .await
            .unwrap(),
        vec![7; 11]
    );
    assert_eq!(stats.snapshot().write_stream_bytes, 11);
}

#[tokio::test]
async fn read_await_does_not_include_consumer_processing_intervals() {
    let stats = diagnostics();
    let stream: ByteStream = Box::pin(futures_util::stream::iter([
        Ok(Bytes::from_static(b"a")),
        Ok(Bytes::from_static(b"b")),
    ]));
    let mut stream = stats.read_stream(stream);
    stream.next().await.unwrap().unwrap();
    // 不使用实际睡眠，模拟消费者处理时间，验证 next().await 与整个流生命周期分开统计。
    stats.state().read_await = Duration::from_millis(12);
    stream.next().await.unwrap().unwrap();
    assert!(stream.next().await.is_none());
    assert_eq!(stats.snapshot().read_await_ms, 12);
}

#[test]
fn resume_accumulates_without_counting_pause_and_marks_crash_history_as_partial() {
    let stats = diagnostics();
    stats.preparation(
        Duration::from_millis(12),
        Duration::from_millis(3),
        Duration::from_millis(4),
    );
    stats.begin();
    stats.add_active(Duration::from_millis(40));
    stats.finish("paused");
    let before = stats.snapshot();
    let resumed = Diagnostics::restore(before.clone());
    assert_eq!(resumed.snapshot().active_ms, before.active_ms);
    assert!(!resumed.snapshot().incomplete_history);
    resumed.begin();
    resumed.completed(2);
    resumed.finish("awaitingConfirmation");
    let report = resumed.snapshot();
    assert_eq!(report.attempts, 2);
    assert_eq!(report.completed_files, 2);
    assert!(report.active_ms >= before.active_ms);
    assert_eq!(
        (
            report.preparation_ms,
            report.derivation_ms,
            report.benchmark_ms
        ),
        (12, 3, 4)
    );

    let mut crash = report;
    crash.outcome = "running".into();
    assert!(Diagnostics::restore(crash).snapshot().incomplete_history);
}

#[test]
fn phases_and_operations_count_failures_and_keep_only_ten_anonymous_slow_files() {
    let stats = diagnostics();
    {
        let mut phases = stats.phases();
        phases.set("checkingStage");
        phases.set("checkingBackup");
    }
    {
        let mut timer = stats.operation("copy");
        timer.success = true;
    }
    {
        let _timer = stats.operation("copy");
    }
    for index in 0..20 {
        let mut file = stats.file(index, "ctrAttachment", index as u64);
        file.started = Instant::now() - Duration::from_millis(index as u64 * 10);
        file.completed = true;
    }
    let report = stats.snapshot();
    assert_eq!(report.slowest_files.len(), 10);
    assert_eq!(report.slowest_files[0].index, 19);
    assert!(report
        .slowest_files
        .windows(2)
        .all(|w| w[0].elapsed_ms >= w[1].elapsed_ms));
    assert_eq!(
        (report.operations[0].count, report.operations[0].failed),
        (2, 1)
    );
    assert_eq!(
        report
            .phases
            .iter()
            .find(|p| p.name == "checkingStage")
            .unwrap()
            .failed,
        0
    );
    assert_eq!(
        report
            .phases
            .iter()
            .find(|p| p.name == "checkingBackup")
            .unwrap()
            .failed,
        1
    );
}

#[test]
fn submillisecond_crypto_and_hash_samples_are_accumulated_before_rounding() {
    let stats = diagnostics();
    for _ in 0..100 {
        stats.crypto(Duration::from_micros(50));
        stats.hash(Duration::from_micros(30));
    }
    assert_eq!(
        (stats.snapshot().crypto_ms, stats.snapshot().hash_ms),
        (5, 3)
    );
}

#[tokio::test]
async fn measured_ctr_has_identical_output_and_propagates_validation_and_io_errors() {
    let (crypto, _) = super::super::tests::crypto("same-password");
    let nonce = [5; 16];
    let data = vec![0x42; 65537];
    let normal = crypto
        .decrypt_streaming(super::super::storage::byte_stream(data.clone()), &nonce, 0)
        .unwrap();
    let expected = collect_data(normal).await.unwrap();
    let stats = diagnostics();
    let captured = stats.clone();
    let measured = crypto
        .decrypt_streaming_with_timing(
            super::super::storage::byte_stream(data),
            &nonce,
            0,
            move |duration| captured.crypto(duration),
        )
        .unwrap();
    assert_eq!(collect_data(measured).await.unwrap(), expected);
    assert!(stats.state().crypto > Duration::ZERO);
    assert!(crypto
        .decrypt_streaming_with_timing(super::super::storage::byte_stream(vec![]), &[0], 0, |_| {})
        .is_err());
    let stream: ByteStream = Box::pin(futures_util::stream::iter([Err(std::io::Error::other(
        "failed",
    ))]));
    let measured = crypto
        .decrypt_streaming_with_timing(stream, &nonce, 0, |_| {
            panic!("failed input must not be transformed")
        })
        .unwrap();
    assert!(collect_data(measured).await.is_err());
}
