#[cfg(test)]
mod id_generate_tests {
    use crate::utils::id_generate::{
        generate_descending_id, generate_descending_id_with_timestamp,
    };
    use std::collections::HashSet;

    #[test]
    fn preserves_existing_descending_id_semantics() {
        assert_eq!(generate_descending_id_with_timestamp(0), 9_999_999_999_999);
        assert_eq!(generate_descending_id_with_timestamp(1), 9_999_999_999_998);
        assert!(generate_descending_id_with_timestamp(1_750_000_000_000) < 9_999_999_999_999);
    }

    #[test]
    fn generated_ids_are_unique_and_monotonically_descending() {
        let ids: Vec<_> = (0..10_000).map(|_| generate_descending_id()).collect();
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
        assert!(ids.windows(2).all(|pair| pair[0] > pair[1]));
    }

    #[test]
    fn generated_ids_are_unique_across_threads() {
        let handles: Vec<_> = (0..16)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..1_000)
                        .map(|_| generate_descending_id())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let ids: Vec<_> = handles
            .into_iter()
            .flat_map(|handle| handle.join().expect("ID generation thread panicked"))
            .collect();

        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
    }
}
