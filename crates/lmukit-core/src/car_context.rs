//! Stable, platform-independent LMU car-context types.
//!
//! Platform readers belong in the desktop shell. They publish observations
//! through this boundary so profile selection can be tested without a running
//! game or a Windows shared-memory mapping.

use serde::{Deserialize, Serialize};

pub const CLASS_GT3: &str = "GT3";
pub const CLASS_GTE: &str = "GTE";
pub const CLASS_LMP3: &str = "LMP3";
pub const CLASS_LMP2: &str = "LMP2";
pub const CLASS_HY: &str = "HY";

pub fn display_existing_class(class_id: &str) -> &str {
    if class_id.eq_ignore_ascii_case(CLASS_GT3) {
        "LMGT3"
    } else {
        class_id
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LmuCarContextSource {
    Rest,
    SharedMemory,
    Manual,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LmuCarContextStatus {
    Confirmed,
    Stale,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LmuSessionState {
    Menu,
    Loading,
    Session,
    Realtime,
    Replay,
    #[default]
    Unknown,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct LmuCarContext {
    pub game_version: Option<i32>,
    pub vehicle_id: Option<String>,
    pub vehicle_name: Option<String>,
    pub class_raw: Option<String>,
    /// One of LMUKit's existing profile class-tag identifiers.
    pub class_normalized: Option<String>,
    pub session_state: LmuSessionState,
    pub source: LmuCarContextSource,
    /// RFC 3339 UTC timestamp supplied by the platform provider.
    pub observed_at: Option<String>,
    pub status: LmuCarContextStatus,
}

/// Conservatively maps an observed LMU class spelling to the existing
/// profile tag model. Unknown values stay unknown and must not select a
/// profile automatically.
pub fn normalize_existing_class(raw: &str) -> Option<&'static str> {
    let normalized = raw.trim().to_ascii_uppercase();
    match normalized.as_str() {
        "GT3" | "LMGT3" => Some(CLASS_GT3),
        "GTE" => Some(CLASS_GTE),
        "LMP3" | "LMP 3" => Some(CLASS_LMP3),
        "LMP2" | "LMP 2" | "LMP2_ELMS" | "LMP2_WEC" => Some(CLASS_LMP2),
        "HY" | "HYPER" | "HYPERCAR" => Some(CLASS_HY),
        _ => None,
    }
}

pub trait LmuCarContextProvider {
    fn current_context(&mut self) -> anyhow::Result<LmuCarContext>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_verified_aliases_to_existing_profile_tags() {
        let cases = [
            ("GT3", CLASS_GT3),
            (" lmgt3 ", CLASS_GT3),
            ("GTE", CLASS_GTE),
            ("LMP 3", CLASS_LMP3),
            ("lmp2", CLASS_LMP2),
            ("LMP2_ELMS", CLASS_LMP2),
            ("LMP2_WEC", CLASS_LMP2),
            ("Hypercar", CLASS_HY),
            ("Hyper", CLASS_HY),
            ("hy", CLASS_HY),
        ];

        for (raw, expected) in cases {
            assert_eq!(normalize_existing_class(raw), Some(expected), "{raw}");
        }
    }

    #[test]
    fn refuses_to_guess_ambiguous_classes() {
        for raw in ["", "LMH", "LMDh", "GTP", "Porsche 963", "LMP"] {
            assert_eq!(normalize_existing_class(raw), None, "{raw}");
        }
    }

    #[test]
    fn presents_the_legacy_gt3_identifier_as_lmgt3() {
        assert_eq!(display_existing_class(CLASS_GT3), "LMGT3");
        assert_eq!(display_existing_class("gt3"), "LMGT3");
        assert_eq!(display_existing_class(CLASS_HY), CLASS_HY);
    }
}
