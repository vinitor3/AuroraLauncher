//! Optional projections for the explicitly selected durable receipt workflow.
//! Absence preserves the original RAM-only IPC contract byte-for-byte.
use super::VoiceMod;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DurableReceiptCapabilities {
    pub schema_version: u8,
    pub max_receipts: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DurableReceiptState {
    Planned,
    Saved,
    Uncertain,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DurableReceiptProjection {
    pub state: DurableReceiptState,
    pub receipt_id: Option<String>,
    pub generation: Option<u64>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SavedReceiptState {
    Ready,
    Restored,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedReceiptSummary {
    pub receipt_id: String,
    pub voice_mod: VoiceMod,
    pub state: SavedReceiptState,
    pub generation: u64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceiptList {
    pub instance_id: String,
    pub receipts: Vec<SavedReceiptSummary>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListReceiptsRequest {
    pub instance_id: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeReceiptRequest {
    pub instance_id: String,
    pub receipt_id: String,
    pub expected_generation: u64,
}

#[cfg(test)]
mod tests {
    use super::super::{
        PrepareApplyRequest, VoiceAnnouncementError, VoiceAnnouncementManager,
        VoiceAnnouncementSnapshot,
    };
    use super::*;
    use serde_json::json;
    use std::path::Path;

    #[test]
    fn durable_ipc_rejects_paths_generations_and_extra_authority() {
        let id = uuid::Uuid::new_v4().to_string();
        let valid = json!({"instanceId":"diagnostic-copy","receiptId":id,"expectedGeneration":0});
        assert!(serde_json::from_value::<ResumeReceiptRequest>(valid.clone()).is_ok());
        for extra in ["path", "token", "before", "digest", "pid", "voiceMod"] {
            let mut value = valid.clone();
            value[extra] = json!("untrusted");
            assert!(serde_json::from_value::<ResumeReceiptRequest>(value).is_err());
        }
        for generation in [json!(-1), json!(0.5), json!("0"), json!(null)] {
            let mut value = valid.clone();
            value["expectedGeneration"] = generation;
            assert!(serde_json::from_value::<ResumeReceiptRequest>(value).is_err());
        }
        assert!(serde_json::from_value::<ResumeReceiptRequest>(
            json!({"instanceId":"copy","receiptId":id})
        )
        .is_err());
        assert!(serde_json::from_value::<ListReceiptsRequest>(
            json!({"instanceId":"copy","path":"elsewhere"})
        )
        .is_err());
        assert!(serde_json::from_value::<ListReceiptsRequest>(json!({})).is_err());
        assert!(serde_json::from_value::<PrepareApplyRequest>(
            json!({"operationId":id,"generation":0,"durable":true})
        )
        .is_err());
    }

    #[test]
    fn durable_fields_are_optional_and_disabled_build_preserves_the_old_shape() {
        let manager = VoiceAnnouncementManager::default();
        let available = cfg!(all(windows, feature = "connect-voice-lab"));
        let capability = serde_json::to_value(manager.capabilities()).unwrap();
        if available {
            assert_eq!(
                capability["durableReceipts"],
                json!({"schemaVersion":1,"maxReceipts":64})
            );
        } else {
            assert!(capability.get("durableReceipts").is_none());
        }
        let snapshot = serde_json::to_value(VoiceAnnouncementSnapshot::default()).unwrap();
        assert!(snapshot.get("durableReceipt").is_none());
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            serde_json::to_value(DurableReceiptProjection {
                state: DurableReceiptState::Saved,
                receipt_id: Some(id.clone()),
                generation: Some(0)
            })
            .unwrap(),
            json!({"state":"saved","receiptId":id,"generation":0})
        );
    }

    #[test]
    fn durable_new_commands_check_owner_and_default_gate_before_storage() {
        let manager = VoiceAnnouncementManager::default();
        tauri::async_runtime::block_on(async {
            for owner in ["other", "main"] {
                if owner == "main" && cfg!(all(windows, feature = "connect-voice-lab")) {
                    continue;
                }
                let expected = if owner == "main" {
                    VoiceAnnouncementError::Unavailable
                } else {
                    VoiceAnnouncementError::OwnerMismatch
                };
                assert_eq!(
                    manager
                        .list_receipts(
                            owner,
                            Path::new("not-read"),
                            ListReceiptsRequest {
                                instance_id: "copy".into()
                            }
                        )
                        .await
                        .err(),
                    Some(expected)
                );
                assert_eq!(
                    manager
                        .resume_receipt(
                            owner,
                            Path::new("not-read"),
                            ResumeReceiptRequest {
                                instance_id: "copy".into(),
                                receipt_id: "not-used".into(),
                                expected_generation: 0
                            }
                        )
                        .await
                        .err(),
                    Some(expected)
                );
                assert_eq!(
                    manager
                        .prepare_apply_durable(
                            owner,
                            PrepareApplyRequest {
                                operation_id: "not-used".into(),
                                generation: 0
                            }
                        )
                        .await
                        .err(),
                    Some(expected)
                );
            }
        });
    }
}
