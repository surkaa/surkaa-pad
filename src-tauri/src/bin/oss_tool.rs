//! 测试桶管理工具
//!
//! 用法：
//!   cargo run --bin oss_tool -- list [prefix]
//!   cargo run --bin oss_tool -- delete <key>
//!   cargo run --bin oss_tool -- delete-all [prefix]
//!   cargo run --bin oss_tool -- upload <key> <file_path>
//!   cargo run --bin oss_tool -- put <key> <text>
//!   cargo run --bin oss_tool -- download <key> [output_path]
//!   cargo run --bin oss_tool -- head <key>
//!   cargo run --bin oss_tool -- multipart-list [prefix]
//!   cargo run --bin oss_tool -- multipart-parts <key> <upload_id>
//!
//! 需要在 src-tauri/.env 中配置 ALIYUN_KEY、ALIYUN_SECRET、ALIYUN_BUCKET_NAME、ALIYUN_ENDPOINT。

use s3::Bucket;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct ListPartsResult {
    #[serde(rename = "Key")]
    key: String,
    #[serde(rename = "UploadId")]
    upload_id: String,
    #[serde(rename = "NextPartNumberMarker")]
    next_part_number_marker: Option<u32>,
    #[serde(rename = "IsTruncated", default)]
    is_truncated: String,
    #[serde(rename = "Part", default)]
    parts: Vec<MultipartPartInfo>,
}

#[derive(Debug, Deserialize)]
struct MultipartPartInfo {
    #[serde(rename = "PartNumber")]
    part_number: u32,
    #[serde(rename = "LastModified", default)]
    last_modified: String,
    #[serde(rename = "ETag", default)]
    etag: String,
    #[serde(rename = "Size")]
    size: u64,
}

/// 从 .env 文件加载环境变量
fn load_env() {
    let env_path = std::path::Path::new(".env");
    if !env_path.exists() {
        return;
    }
    let content = std::fs::read_to_string(env_path).unwrap_or_default();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let key = key.trim();
            let value = value.trim();
            if std::env::var(key).is_err() {
                std::env::set_var(key, value);
            }
        }
    }
}

fn main() {
    load_env();

    let akid = std::env::var("ALIYUN_KEY").expect("请配置 ALIYUN_KEY");
    let aks = std::env::var("ALIYUN_SECRET").expect("请配置 ALIYUN_SECRET");
    let bucket_name = std::env::var("ALIYUN_BUCKET_NAME").expect("请配置 ALIYUN_BUCKET_NAME");
    let endpoint = std::env::var("ALIYUN_ENDPOINT").expect("请配置 ALIYUN_ENDPOINT");

    let region = s3::Region::Custom {
        region: extract_region(&endpoint),
        endpoint: format_endpoint(&endpoint),
    };
    let credentials =
        s3::creds::Credentials::new(Some(&akid), Some(&aks), None, None, None).unwrap();
    let bucket = Bucket::new(&bucket_name, region, credentials)
        .unwrap()
        .with_service("oss");

    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("help");

    let rt = tokio::runtime::Runtime::new().unwrap();

    match cmd {
        "list" => {
            let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("");
            println!("列出对象 (prefix={:?}):", prefix);
            let mut token = None;
            let mut total = 0;
            loop {
                let (result, _) = rt
                    .block_on(bucket.list_page(prefix.to_string(), None, token, None, None))
                    .unwrap();
                for obj in &result.contents {
                    println!("  {}  ({} bytes)", obj.key, obj.size);
                    total += 1;
                }
                token = result.next_continuation_token.clone();
                if token.is_none() {
                    break;
                }
            }
            println!("共 {} 个对象", total);
        }

        "delete" => {
            let key = args.get(2).expect("用法: oss_tool delete <key>");
            let resp = rt.block_on(bucket.delete_object(key)).unwrap();
            if resp.status_code() < 300 {
                println!("已删除: {}", key);
            } else {
                println!("删除失败: HTTP {}", resp.status_code());
            }
        }

        "delete-all" => {
            let prefix = args.get(2).map(|s| s.as_str()).unwrap_or("");
            println!("删除所有对象 (prefix={:?})...", prefix);
            let mut token = None;
            let mut deleted = 0;
            loop {
                let (result, _) = rt
                    .block_on(bucket.list_page(prefix.to_string(), None, token, None, None))
                    .unwrap();
                for obj in &result.contents {
                    let _ = rt.block_on(bucket.delete_object(&obj.key));
                    deleted += 1;
                    print!(".");
                    std::io::Write::flush(&mut std::io::stdout()).ok();
                }
                token = result.next_continuation_token.clone();
                if token.is_none() {
                    break;
                }
            }
            println!("\n已删除 {} 个对象", deleted);
        }

        "upload" => {
            let key = args
                .get(2)
                .expect("用法: oss_tool upload <key> <file_path>");
            let path = args
                .get(3)
                .expect("用法: oss_tool upload <key> <file_path>");
            let data = std::fs::read(path).expect("无法读取文件");
            let resp = rt.block_on(bucket.put_object(key, &data)).unwrap();
            println!(
                "已上传: {} ({} bytes, HTTP {})",
                key,
                data.len(),
                resp.status_code()
            );
        }

        "put" => {
            let key = args.get(2).expect("用法: oss_tool put <key> <text>");
            let text = args.get(3).expect("用法: oss_tool put <key> <text>");
            let resp = rt
                .block_on(bucket.put_object(key, text.as_bytes()))
                .unwrap();
            println!(
                "已写入: {} ({} bytes, HTTP {})",
                key,
                text.len(),
                resp.status_code()
            );
        }

        "download" => {
            let key = args
                .get(2)
                .expect("用法: oss_tool download <key> [output_path]");
            let response_data = rt.block_on(bucket.get_object(key)).unwrap();
            match args.get(3) {
                Some(path) => {
                    std::fs::write(path, response_data.as_slice()).expect("无法写入文件");
                    println!(
                        "已下载到: {} ({} bytes)",
                        path,
                        response_data.as_slice().len()
                    );
                }
                None => {
                    let data = response_data.as_slice();
                    let text = String::from_utf8_lossy(data);
                    println!("({} bytes)\n{}", data.len(), text);
                }
            }
        }

        "head" => {
            let key = args.get(2).expect("用法: oss_tool head <key>");
            let (result, status) = rt.block_on(bucket.head_object(key)).unwrap();
            println!("状态: HTTP {}", status);
            println!(
                "  etag:          {}",
                result.e_tag.as_deref().unwrap_or("-")
            );
            println!(
                "  content_type:  {}",
                result.content_type.as_deref().unwrap_or("-")
            );
            println!("  content_len:   {:?}", result.content_length);
            println!(
                "  last_modified: {}",
                result.last_modified.as_deref().unwrap_or("-")
            );
        }

        "multipart-list" => {
            let prefix = args.get(2).map(String::as_str);
            println!(
                "列出未完成 Multipart Upload (bucket={}, prefix={:?}):",
                bucket_name,
                prefix.unwrap_or("")
            );
            let (result, status) = rt
                .block_on(bucket.list_multiparts_uploads_page(prefix, None, None, Some(1000)))
                .expect("列出 Multipart Upload 失败");
            if status >= 300 {
                panic!("列出 Multipart Upload 失败: HTTP {status}");
            }

            let mut total_parts = 0usize;
            let mut total_bytes = 0u64;
            for upload in &result.uploads {
                println!("  key:       {}", upload.key);
                println!("  upload_id: {}", upload.id);
                println!("  initiated: {}", upload.initiated);
                match rt.block_on(list_all_parts(&bucket, &upload.key, &upload.id)) {
                    Ok(parts) => {
                        let bytes = parts.iter().map(|part| part.size).sum::<u64>();
                        total_parts += parts.len();
                        total_bytes += bytes;
                        println!("  parts:     {} ({})", parts.len(), format_bytes(bytes));
                    }
                    Err(error) => println!("  parts:     无法读取 ({error})"),
                }
                println!();
            }

            println!("共 {} 个未完成 Upload", result.uploads.len());
            println!(
                "已读取 {} 个 Part，总大小 {}",
                total_parts,
                format_bytes(total_bytes)
            );
            if result.is_truncated {
                println!("警告: Upload 数量超过单页上限 1000，本次只显示第一页");
            }
        }

        "multipart-parts" => {
            let key = args
                .get(2)
                .expect("用法: oss_tool multipart-parts <key> <upload_id>");
            let upload_id = args
                .get(3)
                .expect("用法: oss_tool multipart-parts <key> <upload_id>");
            println!("列出 Multipart Part:");
            println!("  bucket:    {bucket_name}");
            println!("  key:       {key}");
            println!("  upload_id: {upload_id}");

            let parts = rt
                .block_on(list_all_parts(&bucket, key, upload_id))
                .expect("列出 Multipart Part 失败");
            let total_bytes = parts.iter().map(|part| part.size).sum::<u64>();
            for part in &parts {
                println!(
                    "  part {:>5}: {:>12}  modified={}  etag={}",
                    part.part_number,
                    format_bytes(part.size),
                    part.last_modified,
                    part.etag
                );
            }
            println!(
                "共 {} 个 Part，总大小 {}",
                parts.len(),
                format_bytes(total_bytes)
            );
        }

        "help" => print_help(),
        unknown => {
            eprintln!("未知命令: {unknown}");
            print_help();
        }
    }
}

fn print_help() {
    println!("测试桶管理工具");
    println!();
    println!("用法:");
    println!("  oss_tool list [prefix]           列出对象");
    println!("  oss_tool delete <key>            删除单个对象");
    println!("  oss_tool delete-all [prefix]     删除所有对象（危险！）");
    println!("  oss_tool upload <key> <file>     上传文件");
    println!("  oss_tool put <key> <text>        写入/覆盖文本（模拟外部修改）");
    println!("  oss_tool download <key> [file]   下载对象");
    println!("  oss_tool head <key>              查看对象元数据");
    println!("  oss_tool multipart-list [prefix] 只读列出未完成 Upload 及 Part 汇总");
    println!("  oss_tool multipart-parts <key> <upload-id>");
    println!("                                    只读列出指定 Upload 的 Part 明细");
}

async fn list_all_parts(
    bucket: &Bucket,
    key: &str,
    upload_id: &str,
) -> Result<Vec<MultipartPartInfo>, String> {
    let client = reqwest::Client::new();
    let mut parts = Vec::new();
    let mut marker: Option<u32> = None;

    loop {
        let mut queries = HashMap::from([
            ("uploadId".to_string(), upload_id.to_string()),
            ("max-parts".to_string(), "1000".to_string()),
        ]);
        if let Some(marker) = marker {
            queries.insert("part-number-marker".to_string(), marker.to_string());
        }
        let url = bucket
            .presign_get(key, 60, Some(queries))
            .await
            .map_err(|error| format!("请求签名失败: {error}"))?;
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|error| format!("请求失败: {}", error.without_url()))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(|error| format!("读取响应失败: {}", error.without_url()))?;
        if !status.is_success() {
            return Err(format!("HTTP {status}: {}", body.trim()));
        }

        let page: ListPartsResult =
            quick_xml::de::from_str(&body).map_err(|error| format!("解析响应失败: {error}"))?;
        if page.key != key || page.upload_id != upload_id {
            return Err("OSS 返回的 key 或 upload ID 与请求不一致".to_string());
        }
        let is_truncated = page.is_truncated.eq_ignore_ascii_case("true");
        marker = page.next_part_number_marker;
        parts.extend(page.parts);

        if !is_truncated {
            break;
        }
        if marker.is_none() {
            return Err("OSS 响应已截断但缺少 NextPartNumberMarker".to_string());
        }
    }

    parts.sort_by_key(|part| part.part_number);
    Ok(parts)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut size = bytes as f64;
    let mut unit = 0usize;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{size:.2} {}", UNITS[unit])
    }
}

fn extract_region(endpoint: &str) -> String {
    let host = endpoint
        .strip_prefix("https://")
        .or_else(|| endpoint.strip_prefix("http://"))
        .unwrap_or(endpoint)
        .split('/')
        .next()
        .unwrap_or(endpoint);
    if let Some(rest) = host.strip_prefix("oss-") {
        if let Some(region) = rest.strip_suffix(".aliyuncs.com") {
            return region.to_string();
        }
    }
    "cn-hangzhou".to_string()
}

fn format_endpoint(endpoint: &str) -> String {
    if endpoint.starts_with("http") {
        endpoint.to_string()
    } else {
        format!("https://{}", endpoint)
    }
}

#[cfg(test)]
mod tests {
    use super::{format_bytes, ListPartsResult};

    #[test]
    fn parses_aliyun_list_parts_response() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<ListPartsResult>
  <Bucket>surkaa</Bucket>
  <Key>123/att-file</Key>
  <UploadId>upload-1</UploadId>
  <NextPartNumberMarker>2</NextPartNumberMarker>
  <IsTruncated>false</IsTruncated>
  <Part>
    <PartNumber>1</PartNumber>
    <LastModified>2026-07-27T01:02:03.000Z</LastModified>
    <ETag>etag-1</ETag>
    <Size>8388608</Size>
  </Part>
  <Part>
    <PartNumber>2</PartNumber>
    <LastModified>2026-07-27T01:02:04.000Z</LastModified>
    <ETag>etag-2</ETag>
    <Size>1024</Size>
  </Part>
</ListPartsResult>"#;

        let result: ListPartsResult = quick_xml::de::from_str(xml).unwrap();
        assert_eq!(result.key, "123/att-file");
        assert_eq!(result.upload_id, "upload-1");
        assert_eq!(result.parts.len(), 2);
        assert_eq!(result.parts[0].part_number, 1);
        assert_eq!(result.parts[0].size, 8 * 1024 * 1024);
        assert_eq!(
            format_bytes(result.parts.iter().map(|part| part.size).sum()),
            "8.00 MiB"
        );
    }
}
