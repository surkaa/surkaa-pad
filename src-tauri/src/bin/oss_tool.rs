/// 测试桶管理工具
///
/// 用法：
///   cargo run --bin oss_tool -- list [prefix]
///   cargo run --bin oss_tool -- delete <key>
///   cargo run --bin oss_tool -- delete-all [prefix]
///   cargo run --bin oss_tool -- upload <key> <file_path>
///   cargo run --bin oss_tool -- put <key> <text>
///   cargo run --bin oss_tool -- download <key> [output_path]
///   cargo run --bin oss_tool -- head <key>
///
/// 需要在 src-tauri/.env 中配置 ALIYUN_KEY, ALIYUN_SECRET, ALIYUN_BUCKET_NAME, ALIYUN_ENDPOINT
use s3::Bucket;

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
