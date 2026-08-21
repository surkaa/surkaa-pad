use serde::{de, Deserialize, Deserializer, Serialize};
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

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Type, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CoordinateSystem {
    /// 世界大地测量系统 1984；日记持久化统一保存这一原始坐标。
    Wgs84,
}

#[derive(Serialize, Clone, Debug, Type, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiaryLocation {
    pub coordinate_system: CoordinateSystem,
    pub latitude: f64,
    pub longitude: f64,
    pub horizontal_accuracy_meters: Option<f64>,
    #[specta(type = f64)]
    pub captured_at: i64,
    pub place_name: Option<String>,
    pub altitude_meters: Option<f64>,
    pub vertical_accuracy_meters: Option<f64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UnvalidatedDiaryLocation {
    coordinate_system: CoordinateSystem,
    latitude: f64,
    longitude: f64,
    horizontal_accuracy_meters: Option<f64>,
    captured_at: i64,
    place_name: Option<String>,
    altitude_meters: Option<f64>,
    vertical_accuracy_meters: Option<f64>,
}

impl<'de> Deserialize<'de> for DiaryLocation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let location = UnvalidatedDiaryLocation::deserialize(deserializer)?;
        Self::try_from(location).map_err(de::Error::custom)
    }
}

impl TryFrom<UnvalidatedDiaryLocation> for DiaryLocation {
    type Error = &'static str;

    fn try_from(value: UnvalidatedDiaryLocation) -> Result<Self, Self::Error> {
        if !value.latitude.is_finite() || !(-90.0..=90.0).contains(&value.latitude) {
            return Err("location latitude must be finite and between -90 and 90 degrees");
        }
        if !value.longitude.is_finite() || !(-180.0..=180.0).contains(&value.longitude) {
            return Err("location longitude must be finite and between -180 and 180 degrees");
        }
        validate_optional_accuracy(
            value.horizontal_accuracy_meters,
            "location horizontal accuracy must be finite and non-negative",
        )?;
        validate_optional_accuracy(
            value.vertical_accuracy_meters,
            "location vertical accuracy must be finite and non-negative",
        )?;
        if value
            .altitude_meters
            .is_some_and(|altitude| !altitude.is_finite())
        {
            return Err("location altitude must be finite");
        }
        if value.captured_at < 0 {
            return Err("location captured time must be non-negative");
        }

        Ok(Self {
            coordinate_system: value.coordinate_system,
            latitude: value.latitude,
            longitude: value.longitude,
            horizontal_accuracy_meters: value.horizontal_accuracy_meters,
            captured_at: value.captured_at,
            place_name: value.place_name,
            altitude_meters: value.altitude_meters,
            vertical_accuracy_meters: value.vertical_accuracy_meters,
        })
    }
}

fn validate_optional_accuracy(
    accuracy: Option<f64>,
    error: &'static str,
) -> Result<(), &'static str> {
    if accuracy.is_some_and(|accuracy| !accuracy.is_finite() || accuracy < 0.0) {
        return Err(error);
    }
    Ok(())
}

#[derive(Deserialize, Serialize, Clone, Debug, Type, PartialEq)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum DiaryContentNode {
    Markdown {
        text: String,
    },
    Summary {
        summary: String,
        content: String,
    },
    Location {
        location: DiaryLocation,
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

#[derive(Deserialize, Serialize, Clone, Debug, Type, PartialEq, Default)]
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
        let mut text = String::new();
        for node in &self.nodes {
            match node {
                DiaryContentNode::Markdown { text: markdown } => text.push_str(markdown),
                DiaryContentNode::Summary { summary, content } => {
                    text.push_str(summary);
                    text.push('\n');
                    text.push_str(content);
                }
                DiaryContentNode::Location { location } => {
                    if let Some(place_name) = &location.place_name {
                        text.push_str(place_name);
                    }
                }
                _ => {}
            }
        }
        text
    }

    pub fn title(&self) -> String {
        self.nodes
            .iter()
            .filter_map(|node| match node {
                DiaryContentNode::Markdown { text } => Some(text),
                DiaryContentNode::Summary { summary, .. } => Some(summary),
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
                DiaryContentNode::Markdown { .. }
                | DiaryContentNode::Summary { .. }
                | DiaryContentNode::Location { .. } => {}
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
            DiaryContentNode::Markdown { .. }
            | DiaryContentNode::Summary { .. }
            | DiaryContentNode::Location { .. } => true,
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
        AlbumDisplayMode, CoordinateSystem, DiaryAttachmentCounts, DiaryContent, DiaryContentNode,
        DiaryLocation, ImageSize,
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
    fn summary_text_is_searchable_and_can_supply_the_title() {
        let content = DiaryContent {
            nodes: vec![DiaryContentNode::Summary {
                summary: "外显标题".to_string(),
                content: "折叠的内部文字".to_string(),
            }],
        };

        assert_eq!(content.searchable_text(), "外显标题\n折叠的内部文字");
        assert_eq!(content.title(), "外显标题");
    }

    #[test]
    fn location_place_name_is_searchable_but_does_not_supply_the_title() {
        let content = DiaryContent {
            nodes: vec![
                DiaryContentNode::Location {
                    location: DiaryLocation {
                        coordinate_system: CoordinateSystem::Wgs84,
                        latitude: 23.1291,
                        longitude: 113.2644,
                        horizontal_accuracy_meters: Some(18.5),
                        captured_at: 1_787_392_800_000,
                        place_name: Some("广州市越秀区".to_string()),
                        altitude_meters: None,
                        vertical_accuracy_meters: None,
                    },
                },
                DiaryContentNode::Markdown {
                    text: "日记标题".to_string(),
                },
            ],
        };

        assert_eq!(content.searchable_text(), "广州市越秀区日记标题");
        assert_eq!(content.title(), "日记标题");
    }

    #[test]
    fn location_round_trip_uses_explicit_wgs84_fields() {
        let json = serde_json::json!({
            "type": "location",
            "location": {
                "coordinateSystem": "wgs84",
                "latitude": 23.1291,
                "longitude": 113.2644,
                "horizontalAccuracyMeters": 18.5,
                "capturedAt": 1_787_392_800_000_i64,
                "placeName": "广州市越秀区",
                "altitudeMeters": 12.3,
                "verticalAccuracyMeters": 4.5
            }
        });

        let node: DiaryContentNode = serde_json::from_value(json.clone()).unwrap();
        assert!(matches!(
            &node,
            DiaryContentNode::Location { location }
                if location.coordinate_system == CoordinateSystem::Wgs84
                    && location.latitude == 23.1291
                    && location.longitude == 113.2644
                    && location.horizontal_accuracy_meters == Some(18.5)
                    && location.place_name.as_deref() == Some("广州市越秀区")
        ));
        assert_eq!(serde_json::to_value(node).unwrap(), json);
    }

    #[test]
    fn rejects_invalid_location_values_during_deserialization() {
        let valid = serde_json::json!({
            "coordinateSystem": "wgs84",
            "latitude": 23.1291,
            "longitude": 113.2644,
            "horizontalAccuracyMeters": 18.5,
            "capturedAt": 1,
            "placeName": null,
            "altitudeMeters": null,
            "verticalAccuracyMeters": null
        });

        for (field, invalid_value) in [
            ("latitude", serde_json::json!(90.1)),
            ("longitude", serde_json::json!(-180.1)),
            ("horizontalAccuracyMeters", serde_json::json!(-0.1)),
            ("verticalAccuracyMeters", serde_json::json!(-0.1)),
            ("capturedAt", serde_json::json!(-1)),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = invalid_value;
            assert!(
                serde_json::from_value::<DiaryLocation>(invalid).is_err(),
                "field {field} should be rejected"
            );
        }
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
