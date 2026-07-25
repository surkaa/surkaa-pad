use serde::{Deserialize, Serialize};
use specta::Type;

#[derive(Deserialize, Serialize, Clone, Debug, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ImageSize {
    Normal,
    Small,
}

#[derive(Deserialize, Serialize, Clone, Debug, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AlbumDisplayMode {
    HorizontalList,
    StackedCards,
}

#[derive(Deserialize, Serialize, Clone, Debug, Type, PartialEq, Eq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DiaryContentNode {
    Markdown {
        text: String,
    },
    Image {
        attachment_id: String,
        size: ImageSize,
    },
    Video {
        attachment_id: String,
    },
    Audio {
        attachment_id: String,
    },
    File {
        attachment_id: String,
    },
    Album {
        id: String,
        attachment_ids: Vec<String>,
        #[serde(rename = "displayMode")]
        #[specta(rename = "displayMode")]
        display_mode: AlbumDisplayMode,
    },
}

#[derive(Deserialize, Serialize, Clone, Debug, Type, PartialEq, Eq, Default)]
pub struct DiaryContent {
    pub nodes: Vec<DiaryContentNode>,
}

impl DiaryContent {
    pub fn from_editor_text(text: &str) -> Self {
        let mut nodes = Vec::new();
        let mut text_start = 0;
        let mut search_start = 0;

        while let Some(relative_start) = text[search_start..].find("[[") {
            let marker_start = search_start + relative_start;
            let marker_body_start = marker_start + 2;
            let Some(relative_end) = text[marker_body_start..].find("]]") else {
                break;
            };
            let marker_end = marker_body_start + relative_end;
            let marker_body = &text[marker_body_start..marker_end];

            let Some(node) = parse_attachment_marker(marker_body) else {
                search_start = marker_body_start;
                continue;
            };

            push_markdown(&mut nodes, &text[text_start..marker_start]);
            nodes.push(node);
            text_start = marker_end + 2;
            search_start = text_start;
        }

        push_markdown(&mut nodes, &text[text_start..]);
        Self { nodes }
    }

    pub fn searchable_text(&self) -> String {
        self.nodes
            .iter()
            .filter_map(|node| match node {
                DiaryContentNode::Markdown { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    pub fn title(&self) -> String {
        self.searchable_text()
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .to_string()
    }

    pub fn remove_attachment(&mut self, attachment_id: &str) {
        self.nodes.retain_mut(|node| match node {
            DiaryContentNode::Image {
                attachment_id: node_attachment_id,
                ..
            }
            | DiaryContentNode::Video {
                attachment_id: node_attachment_id,
            }
            | DiaryContentNode::Audio {
                attachment_id: node_attachment_id,
            }
            | DiaryContentNode::File {
                attachment_id: node_attachment_id,
            } => node_attachment_id != attachment_id,
            DiaryContentNode::Album { attachment_ids, .. } => {
                attachment_ids.retain(|id| id != attachment_id);
                !attachment_ids.is_empty()
            }
            DiaryContentNode::Markdown { .. } => true,
        });
    }

    /// 将仅由空白分隔的连续图片合并为图集，供 V2 → V3 迁移使用。
    pub fn group_consecutive_images_into_albums(&mut self) {
        let mut grouped = Vec::with_capacity(self.nodes.len());
        let mut index = 0;
        let mut album_index = 1;

        while index < self.nodes.len() {
            let DiaryContentNode::Image { attachment_id, .. } = &self.nodes[index] else {
                grouped.push(self.nodes[index].clone());
                index += 1;
                continue;
            };

            let mut attachment_ids = vec![attachment_id.clone()];
            let mut cursor = index + 1;
            while cursor < self.nodes.len() {
                match &self.nodes[cursor] {
                    DiaryContentNode::Image { attachment_id, .. } => {
                        attachment_ids.push(attachment_id.clone());
                        cursor += 1;
                    }
                    DiaryContentNode::Markdown { text }
                        if text.chars().all(char::is_whitespace)
                            && matches!(
                                self.nodes.get(cursor + 1),
                                Some(DiaryContentNode::Image { .. })
                            ) =>
                    {
                        if let DiaryContentNode::Image { attachment_id, .. } =
                            &self.nodes[cursor + 1]
                        {
                            attachment_ids.push(attachment_id.clone());
                        }
                        cursor += 2;
                    }
                    _ => break,
                }
            }

            if attachment_ids.len() >= 2 {
                grouped.push(DiaryContentNode::Album {
                    id: format!("migration-v3-album-{album_index}"),
                    attachment_ids,
                    display_mode: AlbumDisplayMode::HorizontalList,
                });
                album_index += 1;
                index = cursor;
            } else {
                grouped.push(self.nodes[index].clone());
                index += 1;
            }
        }

        self.nodes = grouped;
    }
}

impl From<&str> for DiaryContent {
    fn from(value: &str) -> Self {
        Self::from_editor_text(value)
    }
}

impl From<String> for DiaryContent {
    fn from(value: String) -> Self {
        Self::from_editor_text(&value)
    }
}

fn push_markdown(nodes: &mut Vec<DiaryContentNode>, text: &str) {
    if !text.is_empty() {
        nodes.push(DiaryContentNode::Markdown {
            text: text.to_string(),
        });
    }
}

fn parse_attachment_marker(marker: &str) -> Option<DiaryContentNode> {
    let (kind, value) = marker.split_once(':')?;
    let mut parts = value.split('|');
    let attachment_id = parts.next()?.to_string();
    if attachment_id.is_empty() {
        return None;
    }

    match kind {
        "IMG" => {
            let size = if parts.any(|config| config == "size=small") {
                ImageSize::Small
            } else {
                ImageSize::Normal
            };
            Some(DiaryContentNode::Image {
                attachment_id,
                size,
            })
        }
        "VID" => Some(DiaryContentNode::Video { attachment_id }),
        "AUD" => Some(DiaryContentNode::Audio { attachment_id }),
        "FILE" => Some(DiaryContentNode::File { attachment_id }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{AlbumDisplayMode, DiaryContent, DiaryContentNode, ImageSize};

    #[test]
    fn parses_markdown_and_attachment_nodes_in_order() {
        let content = DiaryContent::from_editor_text(
            "开头\n\n[[IMG:1.jpg|size=small]]\n\n[[FILE:a.pdf]]\n\n结尾",
        );

        assert_eq!(content.nodes.len(), 5);
        assert!(matches!(
            &content.nodes[1],
            DiaryContentNode::Image { attachment_id, size }
                if attachment_id == "1.jpg" && *size == ImageSize::Small
        ));
        assert!(matches!(
            &content.nodes[3],
            DiaryContentNode::File { attachment_id } if attachment_id == "a.pdf"
        ));
        assert_eq!(content.searchable_text(), "开头\n\n\n\n\n\n结尾");
        assert_eq!(content.title(), "开头");
    }

    #[test]
    fn keeps_unknown_or_incomplete_markers_as_markdown() {
        let text = "a [[UNKNOWN:x]] b [[IMG:broken";
        let content = DiaryContent::from_editor_text(text);

        assert_eq!(
            content.nodes,
            vec![DiaryContentNode::Markdown {
                text: text.to_string()
            }]
        );
    }

    #[test]
    fn removes_attachment_references() {
        let mut content =
            DiaryContent::from_editor_text("[[IMG:1.jpg]][[FILE:1.jpg]][[VID:2.mp4]]");
        content.remove_attachment("1.jpg");

        assert_eq!(
            content.nodes,
            vec![DiaryContentNode::Video {
                attachment_id: "2.mp4".to_string()
            }]
        );
    }

    #[test]
    fn serializes_album_fields_as_camel_case() {
        let content = DiaryContent {
            nodes: vec![DiaryContentNode::Album {
                id: "album-1".to_string(),
                attachment_ids: vec!["1.jpg".to_string()],
                display_mode: super::AlbumDisplayMode::StackedCards,
            }],
        };
        let json = serde_json::to_value(content).unwrap();

        assert_eq!(json["nodes"][0]["displayMode"], "stackedCards");
        assert!(json["nodes"][0].get("display_mode").is_none());
    }

    #[test]
    fn groups_images_separated_only_by_whitespace() {
        let mut content = DiaryContent::from_editor_text(
            "before\n[[IMG:1.jpg]] \n\t [[IMG:2.jpg]][[IMG:3.jpg]]\nafter",
        );
        content.group_consecutive_images_into_albums();

        assert_eq!(
            content.nodes,
            vec![
                DiaryContentNode::Markdown {
                    text: "before\n".to_string(),
                },
                DiaryContentNode::Album {
                    id: "migration-v3-album-1".to_string(),
                    attachment_ids: vec![
                        "1.jpg".to_string(),
                        "2.jpg".to_string(),
                        "3.jpg".to_string(),
                    ],
                    display_mode: AlbumDisplayMode::HorizontalList,
                },
                DiaryContentNode::Markdown {
                    text: "\nafter".to_string(),
                },
            ]
        );
    }

    #[test]
    fn does_not_group_images_separated_by_content_or_other_attachments() {
        let mut content = DiaryContent::from_editor_text(
            "[[IMG:1.jpg]] text [[IMG:2.jpg]][[FILE:a.pdf]][[IMG:3.jpg]]",
        );
        let original = content.clone();
        content.group_consecutive_images_into_albums();

        assert_eq!(content, original);
    }

    #[test]
    fn preserves_single_images_and_their_surrounding_whitespace() {
        let source = "before \n[[IMG:only.jpg|size=small]]\n\t after";
        let mut content = DiaryContent::from_editor_text(source);
        let original = content.clone();

        content.group_consecutive_images_into_albums();

        assert_eq!(content, original);
        assert!(matches!(
            &content.nodes[1],
            DiaryContentNode::Image { attachment_id, size }
                if attachment_id == "only.jpg" && *size == ImageSize::Small
        ));
    }

    #[test]
    fn creates_multiple_albums_with_stable_sequential_ids() {
        let source = concat!(
            "[[IMG:1.jpg]][[IMG:2.jpg]]",
            "separator",
            "[[IMG:3.jpg]]\n[[IMG:4.jpg]]\t[[IMG:5.jpg]]",
        );
        let mut first = DiaryContent::from_editor_text(source);
        let mut second = DiaryContent::from_editor_text(source);

        first.group_consecutive_images_into_albums();
        second.group_consecutive_images_into_albums();

        assert_eq!(first, second, "相同 V2 内容必须产生稳定的迁移结果");
        assert!(matches!(
            &first.nodes[0],
            DiaryContentNode::Album { id, attachment_ids, .. }
                if id == "migration-v3-album-1" && attachment_ids == &["1.jpg", "2.jpg"]
        ));
        assert!(matches!(
            &first.nodes[2],
            DiaryContentNode::Album { id, attachment_ids, .. }
                if id == "migration-v3-album-2"
                    && attachment_ids == &["3.jpg", "4.jpg", "5.jpg"]
        ));
    }

    #[test]
    fn recognizes_rust_whitespace_but_not_zero_width_space() {
        let mut whitespace =
            DiaryContent::from_editor_text("[[IMG:1.jpg]]\u{00a0}\u{3000}\r\n[[IMG:2.jpg]]");
        whitespace.group_consecutive_images_into_albums();
        assert!(matches!(
            whitespace.nodes.as_slice(),
            [DiaryContentNode::Album { attachment_ids, .. }]
                if attachment_ids == &["1.jpg", "2.jpg"]
        ));

        let mut zero_width = DiaryContent::from_editor_text("[[IMG:1.jpg]]\u{200b}[[IMG:2.jpg]]");
        let original = zero_width.clone();
        zero_width.group_consecutive_images_into_albums();
        assert_eq!(
            zero_width, original,
            "零宽空格不是 Rust whitespace，不应被吞掉"
        );
    }

    #[test]
    fn every_non_image_node_breaks_a_group() {
        for marker in [
            "[[FILE:a.pdf]]",
            "[[VID:a.mp4]]",
            "[[AUD:a.mp3]]",
            "[[UNKNOWN:value]]",
        ] {
            let source = format!("[[IMG:1.jpg]]{marker}[[IMG:2.jpg]]");
            let mut content = DiaryContent::from_editor_text(&source);
            content.group_consecutive_images_into_albums();

            assert!(
                content
                    .nodes
                    .iter()
                    .all(|node| !matches!(node, DiaryContentNode::Album { .. })),
                "{marker} 必须中断图片分组"
            );
        }
    }

    #[test]
    fn grouping_is_idempotent() {
        let mut content =
            DiaryContent::from_editor_text("[[IMG:1.jpg]]\n[[IMG:2.jpg]]\n[[IMG:3.jpg]]");
        content.group_consecutive_images_into_albums();
        let once = content.clone();

        content.group_consecutive_images_into_albums();

        assert_eq!(content, once);
    }
}
