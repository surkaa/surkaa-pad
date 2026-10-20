use super::*;
use crate::cryptos::crypto_types::EncryptionAlgorithm::Ctr;
use std::sync::Mutex;

fn attachment() -> AttachmentMeta {
    AttachmentMeta {
        id: "file".into(),
        filename: "DATA.JSONL".into(),
        mimetype: "application/octet-stream".into(),
        size: 0,
        encrypted: false,
        nonce: vec![],
        algorithm: Ctr,
        etag: Some("etag".into()),
        content_info: None,
    }
}

fn cursor(size: usize) -> Cursor {
    parse_cursor(None, "diary", &attachment(), size as u64).unwrap()
}

async fn page(
    data: &[u8],
    cursor: Cursor,
    requests: &Mutex<Vec<(u64, u64)>>,
) -> Result<JsonlPage, ServerError> {
    read_page(cursor, |start, end| {
        requests.lock().unwrap().push((start, end));
        std::future::ready(Ok(data[start as usize..=end as usize].to_vec()))
    })
    .await
}

#[tokio::test]
async fn paginates_ten_lines_without_restarting_or_loading_the_whole_file() {
    let text = (0..10_000)
        .map(|i| format!("{{\"id\":{i}}}\n"))
        .collect::<String>();
    let requests = Mutex::new(Vec::new());
    let first = page(text.as_bytes(), cursor(text.len()), &requests)
        .await
        .unwrap();
    assert_eq!(first.lines.len(), 10);
    assert_eq!(first.lines[9].line_number, 10);
    assert_eq!(first.lines[9].text, "{\"id\":9}");
    assert_eq!(requests.lock().unwrap()[0], (0, READ_CHUNK_BYTES - 1));
    let next = parse_cursor(
        Some(&format!("cursor={}", first.next_cursor.unwrap())),
        "diary",
        &attachment(),
        text.len() as u64,
    )
    .unwrap();
    let offset = next.offset;
    let second = page(text.as_bytes(), next, &requests).await.unwrap();
    assert_eq!(second.lines[0].line_number, 11);
    assert_eq!(second.lines[0].text, "{\"id\":10}");
    assert_eq!(requests.lock().unwrap()[1].0, offset - 1);
    assert!(second.next_cursor.is_some());
}

#[tokio::test]
async fn handles_empty_files_crlf_utf8_bom_blank_lines_and_last_line_without_newline() {
    let requests = Mutex::new(Vec::new());
    let empty = page(b"", cursor(0), &requests).await.unwrap();
    assert!(empty.lines.is_empty());
    assert!(empty.next_cursor.is_none());
    assert!(requests.lock().unwrap().is_empty());
    let data = "\u{feff}{\"text\":\"中文\"}\r\n\r\nnull\ntrue\n\"末行\"".as_bytes();
    let result = page(data, cursor(data.len()), &requests).await.unwrap();
    assert_eq!(
        result
            .lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>(),
        ["{\"text\":\"中文\"}", "", "null", "true", "\"末行\""]
    );
    assert_eq!(result.lines[4].line_number, 5);
    assert!(result.next_cursor.is_none());
    let trailing = page(b"1\n2\n", cursor(4), &requests).await.unwrap();
    assert_eq!(trailing.lines.len(), 2);
}

#[tokio::test]
async fn preserves_long_lines_and_utf8_characters_across_range_boundaries() {
    let text = format!("\"{}\"\r\n{{\"next\":true}}", "中".repeat(30_000));
    let requests = Mutex::new(Vec::new());
    let result = page(text.as_bytes(), cursor(text.len()), &requests)
        .await
        .unwrap();
    assert_eq!(result.lines[0].text, format!("\"{}\"", "中".repeat(30_000)));
    assert_eq!(result.lines[1].text, "{\"next\":true}");
    assert_eq!(requests.lock().unwrap().len(), 2);
    assert!(result.next_cursor.is_none());
}

#[tokio::test]
async fn restores_all_records_across_many_pages_and_split_crlf() {
    let first = format!("\"{}\"\r\n", "a".repeat(READ_CHUNK_BYTES as usize - 3));
    assert_eq!(first.as_bytes()[READ_CHUNK_BYTES as usize - 1], b'\r');
    let text = first + &(2..=333).map(|i| format!("{i}\n")).collect::<String>();
    let requests = Mutex::new(Vec::new());
    let mut next = cursor(text.len());
    let mut count = 0;
    loop {
        let result = page(text.as_bytes(), next, &requests).await.unwrap();
        assert!(result.lines.len() <= PAGE_LINES);
        for line in result.lines {
            count += 1;
            assert_eq!(line.line_number, count);
            if count != 1 {
                assert_eq!(line.text, count.to_string());
            }
        }
        let Some(encoded) = result.next_cursor else {
            break;
        };
        next = parse_cursor(
            Some(&format!("cursor={encoded}")),
            "diary",
            &attachment(),
            text.len() as u64,
        )
        .unwrap();
    }
    assert_eq!(count, 333);
    for trailing in ["", "\n"] {
        let text = (1..=10)
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join("\n")
            + trailing;
        let result = page(text.as_bytes(), cursor(text.len()), &requests)
            .await
            .unwrap();
        assert_eq!(result.lines.len(), 10);
        assert!(result.next_cursor.is_none());
    }
}

#[tokio::test]
async fn reports_non_utf8_per_line_without_blocking_following_lines() {
    let data = b"\"\xff\"\n{}\n";
    let result = page(data, cursor(data.len()), &Mutex::new(Vec::new()))
        .await
        .unwrap();
    assert!(result.lines[0].error.is_some());
    assert!(result.lines[0].text.contains('\u{fffd}'));
    assert_eq!(result.lines[1].text, "{}");
    assert!(result.lines[1].error.is_none());
}

#[tokio::test]
async fn rejects_oversized_lines_without_scanning_the_remaining_file() {
    let data = vec![b'x'; MAX_LINE_BYTES + READ_CHUNK_BYTES as usize * 2];
    let requests = Mutex::new(Vec::new());
    let error = page(&data, cursor(data.len()), &requests)
        .await
        .unwrap_err();
    assert!(matches!(error, ServerError::BadRequest(message) if message.contains("4 MiB")));
    assert!(requests.lock().unwrap().last().unwrap().1 < data.len() as u64 - 1);
}

#[tokio::test]
async fn page_byte_budget_never_truncates_or_drops_large_json_records() {
    let line = format!("\"{}\"\n", "a".repeat(MAX_LINE_BYTES - 2));
    let text = line.repeat(6);
    let requests = Mutex::new(Vec::new());
    let mut next = cursor(text.len());
    for index in 1..=6 {
        requests.lock().unwrap().clear();
        let result = page(text.as_bytes(), next, &requests).await.unwrap();
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.lines[0].line_number, index);
        assert_eq!(
            result.lines[0].text.as_bytes(),
            &line.as_bytes()[..MAX_LINE_BYTES]
        );
        // 预算正好满时，只允许当前 Range 的少量预读，不能再扫描下一整条大记录。
        assert!(
            requests.lock().unwrap().last().unwrap().1
                < index * line.len() as u64 + READ_CHUNK_BYTES
        );
        if index == 6 {
            assert!(result.next_cursor.is_none());
            break;
        }
        next = parse_cursor(
            Some(&format!("cursor={}", result.next_cursor.unwrap())),
            "diary",
            &attachment(),
            text.len() as u64,
        )
        .unwrap();
    }
}

#[tokio::test]
async fn accepts_exact_four_mib_lines_and_rejects_one_extra_byte_for_all_line_endings() {
    assert_eq!(MAX_LINE_BYTES, 4 * 1024 * 1024);
    for ending in ["", "\n", "\r\n"] {
        let text = format!("\"{}\"{ending}", "a".repeat(MAX_LINE_BYTES - 2));
        let result = page(text.as_bytes(), cursor(text.len()), &Mutex::new(Vec::new()))
            .await
            .unwrap();
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.lines[0].text.len(), MAX_LINE_BYTES);
        assert!(result.lines[0].error.is_none());
        assert!(result.next_cursor.is_none());
        let oversized = format!("\"{}\"{ending}", "a".repeat(MAX_LINE_BYTES - 1));
        assert!(matches!(
            page(oversized.as_bytes(), cursor(oversized.len()), &Mutex::new(Vec::new())).await,
            Err(ServerError::BadRequest(message)) if message.contains("4 MiB")
        ));
    }
}

#[tokio::test]
async fn retains_a_four_mib_line_after_a_smaller_record_without_losing_following_records() {
    let large = format!("\"{}\"", "a".repeat(MAX_LINE_BYTES - 2));
    let text = format!("{{}}\n{large}\ntrue");
    let requests = Mutex::new(Vec::new());
    let mut next = cursor(text.len());
    for expected in ["{}", large.as_str(), "true"] {
        let result = page(text.as_bytes(), next, &requests).await.unwrap();
        assert_eq!(result.lines.len(), 1);
        assert_eq!(result.lines[0].text, expected);
        let Some(encoded) = result.next_cursor else {
            assert_eq!(expected, "true");
            return;
        };
        next = parse_cursor(
            Some(&format!("cursor={encoded}")),
            "diary",
            &attachment(),
            text.len() as u64,
        )
        .unwrap();
    }
    panic!("最后一条记录之后仍然产生了下一页游标");
}

#[tokio::test]
async fn rejects_incomplete_ranges_and_offsets_inside_a_line() {
    let error = read_page(cursor(10), |_, _| std::future::ready(Ok(vec![0])))
        .await
        .unwrap_err();
    assert!(matches!(error, ServerError::Internal(_)));
    let mut cursor = cursor(8);
    cursor.offset = 2;
    cursor.line_number = 2;
    let error = page(b"123\n456\n", cursor, &Mutex::new(Vec::new()))
        .await
        .unwrap_err();
    assert!(matches!(error, ServerError::BadRequest(message) if message.contains("行边界")));
}

#[test]
fn cursor_validation_rejects_malformed_stale_and_cross_attachment_cursors() {
    let encode = |cursor: &Cursor| URL_SAFE_NO_PAD.encode(serde_json::to_vec(cursor).unwrap());
    let mut value = cursor(100);
    value.offset = 30;
    value.line_number = 11;
    let query = format!("cursor={}", encode(&value));
    assert_eq!(
        parse_cursor(Some(&query), "diary", &attachment(), 100)
            .unwrap()
            .offset,
        30
    );
    for query in [
        "cursor=bad".to_owned(),
        "cursor=".into(),
        format!("{query}&{query}"),
        format!("cursor={}", "x".repeat(4097)),
    ] {
        assert!(parse_cursor(Some(&query), "diary", &attachment(), 100).is_err());
    }
    assert!(parse_cursor(Some(&query), "other", &attachment(), 100).is_err());
    assert!(parse_cursor(Some(&query), "diary", &attachment(), 101).is_err());
    let mut changed = attachment();
    changed.etag = Some("new-etag".into());
    assert!(parse_cursor(Some(&query), "diary", &changed, 100).is_err());
    for (offset, line_number) in [(101, 11), (0, 0), (1, 10)] {
        value.offset = offset;
        value.line_number = line_number;
        assert!(parse_cursor(
            Some(&format!("cursor={}", encode(&value))),
            "diary",
            &attachment(),
            100
        )
        .is_err());
    }
}

#[test]
fn recognizes_jsonl_extensions_and_mime_types() {
    let mut file = attachment();
    assert!(is_jsonl_attachment(&file));
    file.filename = "data.ndjson".into();
    assert!(is_jsonl_attachment(&file));
    file.filename = "data.bin".into();
    for mimetype in [
        "Application/JSONL; charset=utf-8",
        "application/x-ndjson",
        "text/jsonl",
    ] {
        file.mimetype = mimetype.into();
        assert!(is_jsonl_attachment(&file));
    }
    file.mimetype = "application/json".into();
    assert!(!is_jsonl_attachment(&file));
}
