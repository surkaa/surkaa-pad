#[cfg(test)]
mod id_generate_tests {
    use crate::utils::id_generate::generate_descending_id;

    #[test]
    fn test() {
        let mut ids = Vec::with_capacity(5);
        for i in 0..5 {
            ids.push((i, generate_descending_id()));
            // 强制等待 1 秒钟，确保下一个时间戳不同
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        // 根据字典序升序排序
        ids.sort_by(|a, b| a.1.cmp(&b.1));
        // 验证排序后 ID 的顺序应该是按照时间倒序的
        for (index, (original_index, id)) in ids.iter().enumerate() {
            println!("原始索引: {}, ID: {}", original_index, id);
            // 期望 ID 的顺序应该是 4, 3, 2, 1, 0
            assert_eq!(*original_index, 4 - index);
        }
    }
}
