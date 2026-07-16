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
        filename: String,
        size: ImageSize,
    },
    Video {
        filename: String,
    },
    Audio {
        filename: String,
    },
    File {
        filename: String,
    },
    Album {
        id: String,
        images: Vec<String>,
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

    pub fn rename_attachment(&mut self, old_filename: &str, new_filename: &str) {
        for node in &mut self.nodes {
            match node {
                DiaryContentNode::Image { filename, .. }
                | DiaryContentNode::Video { filename }
                | DiaryContentNode::Audio { filename }
                | DiaryContentNode::File { filename } => {
                    if filename == old_filename {
                        *filename = new_filename.to_string();
                    }
                }
                DiaryContentNode::Album { images, .. } => {
                    for filename in images {
                        if filename == old_filename {
                            *filename = new_filename.to_string();
                        }
                    }
                }
                DiaryContentNode::Markdown { .. } => {}
            }
        }
    }

    pub fn remove_attachment(&mut self, filename: &str) {
        self.nodes.retain_mut(|node| match node {
            DiaryContentNode::Image {
                filename: node_filename,
                ..
            }
            | DiaryContentNode::Video {
                filename: node_filename,
            }
            | DiaryContentNode::Audio {
                filename: node_filename,
            }
            | DiaryContentNode::File {
                filename: node_filename,
            } => node_filename != filename,
            DiaryContentNode::Album { images, .. } => {
                images.retain(|image| image != filename);
                !images.is_empty()
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
    let filename = parts.next()?.to_string();
    if filename.is_empty() {
        return None;
    }

    match kind {
        "IMG" => {
            let size = if parts.any(|config| config == "size=small") {
                ImageSize::Small
            } else {
                ImageSize::Normal
            };
            Some(DiaryContentNode::Image { filename, size })
        }
        "VID" => Some(DiaryContentNode::Video { filename }),
        "AUD" => Some(DiaryContentNode::Audio { filename }),
        "FILE" => Some(DiaryContentNode::File { filename }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{DiaryContent, DiaryContentNode, ImageSize};

    #[test]
    fn parses_markdown_and_attachment_nodes_in_order() {
        let content = DiaryContent::from_editor_text(
            "开头\n\n[[IMG:1.jpg|size=small]]\n\n[[FILE:a.pdf]]\n\n结尾",
        );

        assert_eq!(content.nodes.len(), 5);
        assert!(matches!(
            &content.nodes[1],
            DiaryContentNode::Image { filename, size }
                if filename == "1.jpg" && *size == ImageSize::Small
        ));
        assert!(matches!(
            &content.nodes[3],
            DiaryContentNode::File { filename } if filename == "a.pdf"
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
    fn renames_and_removes_attachment_references() {
        let mut content =
            DiaryContent::from_editor_text("[[IMG:1.jpg]][[FILE:1.jpg]][[VID:2.mp4]]");
        content.rename_attachment("1.jpg", "renamed.jpg");
        content.remove_attachment("renamed.jpg");

        assert_eq!(
            content.nodes,
            vec![DiaryContentNode::Video {
                filename: "2.mp4".to_string()
            }]
        );
    }

    #[test]
    fn serializes_album_fields_as_camel_case() {
        let content = DiaryContent {
            nodes: vec![DiaryContentNode::Album {
                id: "album-1".to_string(),
                images: vec!["1.jpg".to_string()],
                display_mode: super::AlbumDisplayMode::StackedCards,
            }],
        };
        let json = serde_json::to_value(content).unwrap();

        assert_eq!(json["nodes"][0]["displayMode"], "stackedCards");
        assert!(json["nodes"][0].get("display_mode").is_none());
    }
}
