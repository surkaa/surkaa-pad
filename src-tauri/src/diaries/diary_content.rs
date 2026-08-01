use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashSet;

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
        #[specta(rename = "attachmentId")]
        attachment_id: String,
        size: ImageSize,
    },
    Video {
        #[specta(rename = "attachmentId")]
        attachment_id: String,
    },
    Audio {
        #[specta(rename = "attachmentId")]
        attachment_id: String,
    },
    File {
        #[specta(rename = "attachmentId")]
        attachment_id: String,
    },
    Album {
        id: String,
        #[specta(rename = "attachmentIds")]
        attachment_ids: Vec<String>,
        #[serde(rename = "displayMode")]
        #[specta(rename = "displayMode")]
        display_mode: AlbumDisplayMode,
    },
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Type, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct DiaryAttachmentCounts {
    pub image: u32,
    pub audio: u32,
    pub video: u32,
    pub file: u32,
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

    pub(crate) fn matches_keywords(&self, keywords: &[String], match_any: bool) -> bool {
        if keywords.is_empty() {
            return true;
        }

        let searchable_text = self.searchable_text();
        if match_any {
            keywords
                .iter()
                .any(|keyword| searchable_text.contains(keyword))
        } else {
            keywords
                .iter()
                .all(|keyword| searchable_text.contains(keyword))
        }
    }

    pub fn title(&self) -> String {
        self.nodes
            .iter()
            .filter_map(|node| match node {
                DiaryContentNode::Markdown { text } => Some(text),
                _ => None,
            })
            .flat_map(|text| text.lines())
            .map(str::trim)
            .find(|line| !line.is_empty())
            .unwrap_or("")
            .to_string()
    }

    pub fn attachment_counts(&self) -> DiaryAttachmentCounts {
        self.attachment_counts_matching(|_| true)
    }

    pub(crate) fn attachment_counts_for_ids(
        &self,
        attachment_ids: &HashSet<&str>,
    ) -> DiaryAttachmentCounts {
        self.attachment_counts_matching(|attachment_id| attachment_ids.contains(attachment_id))
    }

    fn attachment_counts_matching(
        &self,
        mut include: impl FnMut(&str) -> bool,
    ) -> DiaryAttachmentCounts {
        let mut counts = DiaryAttachmentCounts::default();

        for node in &self.nodes {
            match node {
                DiaryContentNode::Image { attachment_id, .. } => {
                    if include(attachment_id) {
                        counts.image = counts.image.saturating_add(1);
                    }
                }
                DiaryContentNode::Album { attachment_ids, .. } => {
                    for attachment_id in attachment_ids {
                        if include(attachment_id) {
                            counts.image = counts.image.saturating_add(1);
                        }
                    }
                }
                DiaryContentNode::Audio { attachment_id } => {
                    if include(attachment_id) {
                        counts.audio = counts.audio.saturating_add(1);
                    }
                }
                DiaryContentNode::Video { attachment_id } => {
                    if include(attachment_id) {
                        counts.video = counts.video.saturating_add(1);
                    }
                }
                DiaryContentNode::File { attachment_id } => {
                    if include(attachment_id) {
                        counts.file = counts.file.saturating_add(1);
                    }
                }
                DiaryContentNode::Markdown { .. } => {}
            }
        }

        counts
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
    use super::{
        AlbumDisplayMode, DiaryAttachmentCounts, DiaryContent, DiaryContentNode, ImageSize,
    };
    use std::collections::HashSet;

    #[test]
    fn counts_attachment_nodes_and_each_image_in_an_album() {
        let content = DiaryContent {
            nodes: vec![
                DiaryContentNode::Markdown {
                    text: "正文".to_string(),
                },
                DiaryContentNode::Image {
                    attachment_id: "image-1".to_string(),
                    size: ImageSize::Normal,
                },
                DiaryContentNode::Album {
                    id: "album-1".to_string(),
                    attachment_ids: vec!["image-2".to_string(), "image-3".to_string()],
                    display_mode: AlbumDisplayMode::HorizontalList,
                },
                DiaryContentNode::Audio {
                    attachment_id: "audio-1".to_string(),
                },
                DiaryContentNode::Video {
                    attachment_id: "video-1".to_string(),
                },
                DiaryContentNode::File {
                    attachment_id: "file-1".to_string(),
                },
            ],
        };

        assert_eq!(
            content.attachment_counts(),
            DiaryAttachmentCounts {
                image: 3,
                audio: 1,
                video: 1,
                file: 1,
            }
        );
    }

    #[test]
    fn counts_only_attachment_ids_in_the_requested_set() {
        let content = DiaryContent {
            nodes: vec![
                DiaryContentNode::Image {
                    attachment_id: "plain-image".to_string(),
                    size: ImageSize::Normal,
                },
                DiaryContentNode::Album {
                    id: "album-1".to_string(),
                    attachment_ids: vec![
                        "encrypted-image".to_string(),
                        "plain-album-image".to_string(),
                    ],
                    display_mode: AlbumDisplayMode::StackedCards,
                },
                DiaryContentNode::Audio {
                    attachment_id: "encrypted-audio".to_string(),
                },
                DiaryContentNode::Video {
                    attachment_id: "plain-video".to_string(),
                },
            ],
        };
        let requested = HashSet::from(["encrypted-image", "encrypted-audio", "unused"]);

        assert_eq!(
            content.attachment_counts_for_ids(&requested),
            DiaryAttachmentCounts {
                image: 1,
                audio: 1,
                video: 0,
                file: 0,
            }
        );
    }

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
    fn title_does_not_join_markdown_across_an_album() {
        let content = DiaryContent {
            nodes: vec![
                DiaryContentNode::Markdown {
                    text: "1231".to_string(),
                },
                DiaryContentNode::Album {
                    id: "migration-v3-album-1".to_string(),
                    attachment_ids: vec!["a.jpg".to_string(), "b.jpg".to_string()],
                    display_mode: AlbumDisplayMode::HorizontalList,
                },
                DiaryContentNode::Markdown {
                    text: "test 顺序".to_string(),
                },
            ],
        };

        assert_eq!(content.title(), "1231");
        assert_eq!(content.searchable_text(), "1231test 顺序");
    }

    #[test]
    fn matches_all_or_any_whitespace_separated_keywords() {
        let content = DiaryContent {
            nodes: vec![DiaryContentNode::Markdown {
                text: "上海旅行记录".to_string(),
            }],
        };

        assert!(content.matches_keywords(&["上海".into(), "旅行".into()], false));
        assert!(!content.matches_keywords(&["上海".into(), "北京".into()], false));
        assert!(content.matches_keywords(&["上海".into(), "北京".into()], true));
        assert!(content.matches_keywords(&[], false));
    }

    #[test]
    fn title_uses_the_first_non_empty_markdown_line() {
        let content = DiaryContent {
            nodes: vec![
                DiaryContentNode::Markdown {
                    text: " \n\t".to_string(),
                },
                DiaryContentNode::Image {
                    attachment_id: "a.jpg".to_string(),
                    size: ImageSize::Normal,
                },
                DiaryContentNode::Markdown {
                    text: "  标题  \n正文".to_string(),
                },
            ],
        };

        assert_eq!(content.title(), "标题");
    }

    #[test]
    fn title_is_empty_when_content_has_no_markdown_text() {
        let content = DiaryContent {
            nodes: vec![DiaryContentNode::Image {
                attachment_id: "a.jpg".to_string(),
                size: ImageSize::Normal,
            }],
        };

        assert_eq!(content.title(), "");
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
                display_mode: AlbumDisplayMode::StackedCards,
            }],
        };
        let json = serde_json::to_value(content).unwrap();

        assert_eq!(json["nodes"][0]["displayMode"], "stackedCards");
        assert!(json["nodes"][0].get("display_mode").is_none());
    }
}
