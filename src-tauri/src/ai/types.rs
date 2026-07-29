use serde::Serialize;
use specta::Type;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct AiModel {
    pub id: String,
    pub owned_by: Option<String>,
}
