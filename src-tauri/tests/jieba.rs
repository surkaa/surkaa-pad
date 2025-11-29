#[cfg(test)]
mod jieba_tests {
    use jieba_rs::Jieba;
    use std::collections::HashMap;

    // 辅助函数：执行分词和词频统计的逻辑
    fn get_word_counts(plaintext: &str) -> HashMap<String, i64> {
        // 1. 初始化分词器
        let jieba = Jieba::new();

        // 2. 分词 (使用 Search 模式)
        let tokens = jieba.cut_for_search(plaintext, true);

        // 3. 词频统计
        let mut word_counts: HashMap<String, i64> = HashMap::new();

        for token in tokens {
            let word = token.to_lowercase();
            // 过滤掉纯数字、单个字符和空白/标点符号，避免无效索引
            // 你的原始过滤逻辑：
            // if word.chars().all(|c| c.is_ascii_whitespace() || c.is_ascii_punctuation() || c.is_digit(10)) || word.chars().count() < 2 {
            //     continue;
            // }

            // 稍微简化和改进的过滤逻辑：
            // 1. 至少包含 2 个字符
            if word.chars().count() < 2 {
                continue;
            }
            // 2. 过滤掉全部是空白或标点符号的词（通常分词器不会切出这些，但作为安全措施）
            if word.chars().all(|c| c.is_ascii_whitespace() || c.is_ascii_punctuation()) {
                continue;
            }
            // 3. 过滤掉全部是数字的词
            if word.chars().all(|c| c.is_digit(10)) {
                continue;
            }

            *word_counts.entry(word).or_insert(0) += 1;
        }
        word_counts
    }

    #[test]
    fn test_basic_segmentation_and_counting() {
        let text = "小明硕士毕业于中国科学院计算技术研究所，\
                    后在日本京都大学深造。他是创新技术的领导者。";
        let counts = get_word_counts(text);

        // 检查关键分词是否被统计
        assert_eq!(*counts.get("小明").unwrap_or(&0), 1);
        assert_eq!(*counts.get("硕士").unwrap_or(&0), 1);
        assert_eq!(*counts.get("毕业").unwrap_or(&0), 1);
        assert_eq!(*counts.get("中国").unwrap_or(&0), 1);
        assert_eq!(*counts.get("科学院").unwrap_or(&0), 1);
        assert_eq!(*counts.get("计算").unwrap_or(&0), 1);
        assert_eq!(*counts.get("研究所").unwrap_or(&0), 1);
        assert_eq!(*counts.get("日本").unwrap_or(&0), 1);
        assert_eq!(*counts.get("京都").unwrap_or(&0), 1);
        assert_eq!(*counts.get("深造").unwrap_or(&0), 1);
        assert_eq!(*counts.get("创新").unwrap_or(&0), 1);
        assert_eq!(*counts.get("技术").unwrap_or(&0), 2, "技术应该出现两次");
    }

    #[test]
    fn test_counting_with_repetitions() {
        let text = "测试重复词汇，词汇，测试，测试。";
        let counts = get_word_counts(text);

        assert_eq!(*counts.get("测试").unwrap_or(&0), 3, "测试应该出现三次");
        assert_eq!(*counts.get("词汇").unwrap_or(&0), 2, "词汇应该出现两次");
    }

    #[test]
    fn test_filtering_and_case_insensitivity() {
        let text = "1 2 3. 测试 test Test 测试123。 This is A。";
        let counts = get_word_counts(text);

        // 检查过滤是否生效：纯数字、单个字符、标点符号
        assert!(!counts.contains_key("1"));
        assert!(!counts.contains_key("2"));
        assert!(!counts.contains_key("3"));
        assert!(!counts.contains_key("a")); // 单个字符
        assert!(counts.contains_key("is"));

        assert_eq!(*counts.get("test").unwrap_or(&0), 2);

        // 检查中文和数字混合词
        assert_eq!(*counts.get("测试123").unwrap_or(&0), 0);
        assert_eq!(*counts.get("测试").unwrap_or(&0), 2);

        // 检查总词数，以确保没有多余的被统计
        assert_eq!(counts.len(), 4, "应该只有 '测试', 'this', 'test', 'is' 四个词");
    }

    #[test]
    fn test_empty_input() {
        let text = "";
        let counts = get_word_counts(text);
        assert!(counts.is_empty());
    }
}