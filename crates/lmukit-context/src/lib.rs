//! Read-only LMU car-context decoding and platform provider.

use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail, ensure};
use lmukit_core::car_context::{
    LmuCarContext, LmuCarContextSource, LmuCarContextStatus, LmuSessionState,
    normalize_existing_class,
};

#[cfg(target_os = "windows")]
pub mod windows;

pub const LMU_SHARED_MEMORY_NAME: &str = "LMU_Data";
pub const LMU_SHARED_MEMORY_PAYLOAD_SIZE: usize = 324_820;
const MAX_VEHICLES: usize = 104;
const GAME_VERSION_OFFSET: usize = 64;
const SCORING_OFFSET: usize = 1_632;
const SCORING_SESSION_OFFSET: usize = SCORING_OFFSET + 64;
const SCORING_NUM_VEHICLES_OFFSET: usize = SCORING_OFFSET + 104;
const SCORING_GAME_PHASE_OFFSET: usize = SCORING_OFFSET + 108;
const SCORING_REALTIME_OFFSET: usize = SCORING_OFFSET + 115;
const SCORING_PLAYER_NAME_OFFSET: usize = SCORING_OFFSET + 116;
const VEHICLE_SCORING_OFFSET: usize = 2_192;
const VEHICLE_SCORING_SIZE: usize = 584;
const VEHICLE_DRIVER_OFFSET: usize = 4;
const VEHICLE_NAME_OFFSET: usize = 36;
const VEHICLE_IS_PLAYER_OFFSET: usize = 196;
const VEHICLE_CONTROL_OFFSET: usize = 197;
const VEHICLE_CLASS_OFFSET: usize = 200;
const VEHICLE_FILENAME_OFFSET: usize = 544;

pub fn decode_lmu_car_context(
    snapshot: &[u8],
    observed_at: Option<String>,
) -> Result<LmuCarContext> {
    ensure!(
        snapshot.len() >= LMU_SHARED_MEMORY_PAYLOAD_SIZE,
        "LMU shared-memory snapshot is {} bytes; expected at least {LMU_SHARED_MEMORY_PAYLOAD_SIZE}",
        snapshot.len()
    );

    let view = SnapshotView(snapshot);
    let game_version = view.i32(GAME_VERSION_OFFSET)?;
    ensure!(
        (10_000..100_000).contains(&game_version),
        "LMU shared-memory game version {game_version} is outside the supported format range"
    );
    let session = view.i32(SCORING_SESSION_OFFSET)?;
    ensure!(
        (-1..=20).contains(&session),
        "LMU shared-memory session value {session} is invalid"
    );
    let vehicle_count = view.i32(SCORING_NUM_VEHICLES_OFFSET)?;
    ensure!(
        (0..=MAX_VEHICLES as i32).contains(&vehicle_count),
        "LMU reports invalid scoring vehicle count {vehicle_count}"
    );
    let game_phase = view.u8(SCORING_GAME_PHASE_OFFSET)?;
    ensure!(
        game_phase <= 9,
        "LMU reports invalid game phase {game_phase}"
    );
    let realtime = view.boolean(SCORING_REALTIME_OFFSET)?;
    let player_name = view.c_string(SCORING_PLAYER_NAME_OFFSET, 32)?;

    let mut marked_player = None;
    let mut name_matches = Vec::new();
    for index in 0..vehicle_count as usize {
        let base = VEHICLE_SCORING_OFFSET + index * VEHICLE_SCORING_SIZE;
        let is_player = view.boolean(base + VEHICLE_IS_PLAYER_OFFSET)?;
        if is_player {
            ensure!(
                marked_player.is_none(),
                "LMU reports more than one player scoring row"
            );
            marked_player = Some(base);
        }
        if !player_name.is_empty()
            && view
                .c_string(base + VEHICLE_DRIVER_OFFSET, 32)?
                .eq_ignore_ascii_case(&player_name)
        {
            name_matches.push(base);
        }
    }

    let player_base = marked_player.or_else(|| {
        if name_matches.len() == 1 {
            name_matches.first().copied()
        } else {
            None
        }
    });

    let Some(player_base) = player_base else {
        return Ok(LmuCarContext {
            game_version: Some(game_version),
            session_state: session_state(realtime, game_phase, vehicle_count, None),
            source: LmuCarContextSource::SharedMemory,
            observed_at,
            status: LmuCarContextStatus::Unknown,
            ..LmuCarContext::default()
        });
    };

    let control = view.i8(player_base + VEHICLE_CONTROL_OFFSET)?;
    ensure!(
        (-1..=3).contains(&control),
        "LMU reports invalid player control value {control}"
    );
    let vehicle_name = view.c_string(player_base + VEHICLE_NAME_OFFSET, 64)?;
    let class_raw = view.c_string(player_base + VEHICLE_CLASS_OFFSET, 32)?;
    let vehicle_id = view.c_string(player_base + VEHICLE_FILENAME_OFFSET, 32)?;
    let class_normalized = normalize_existing_class(&class_raw).map(str::to_owned);
    let confirmed = !vehicle_name.is_empty() && !class_raw.is_empty();

    Ok(LmuCarContext {
        game_version: Some(game_version),
        vehicle_id: (!vehicle_id.is_empty()).then_some(vehicle_id),
        vehicle_name: (!vehicle_name.is_empty()).then_some(vehicle_name),
        class_raw: (!class_raw.is_empty()).then_some(class_raw),
        class_normalized,
        session_state: session_state(realtime, game_phase, vehicle_count, Some(control)),
        source: LmuCarContextSource::SharedMemory,
        observed_at,
        status: if confirmed {
            LmuCarContextStatus::Confirmed
        } else {
            LmuCarContextStatus::Unknown
        },
    })
}

pub fn utc_timestamp(time: SystemTime) -> String {
    let elapsed = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let seconds = elapsed.as_secs();
    let seconds_today = seconds % 86_400;
    let hour = seconds_today / 3_600;
    let minute = seconds_today % 3_600 / 60;
    let second = seconds_today % 60;
    let date = date_from_days((seconds / 86_400) as i64);
    format!(
        "{date}T{hour:02}:{minute:02}:{second:02}.{:03}Z",
        elapsed.subsec_millis()
    )
}

fn date_from_days(days: i64) -> String {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

fn session_state(
    realtime: bool,
    game_phase: u8,
    vehicle_count: i32,
    control: Option<i8>,
) -> LmuSessionState {
    if control == Some(3) {
        LmuSessionState::Replay
    } else if realtime {
        LmuSessionState::Realtime
    } else if vehicle_count > 0 || game_phase > 0 {
        LmuSessionState::Session
    } else {
        LmuSessionState::Unknown
    }
}

struct SnapshotView<'a>(&'a [u8]);

impl SnapshotView<'_> {
    fn bytes(&self, offset: usize, length: usize) -> Result<&[u8]> {
        self.0
            .get(offset..offset.saturating_add(length))
            .with_context(|| {
                format!(
                    "LMU field [{offset}:{}] is outside the snapshot",
                    offset + length
                )
            })
    }

    fn u8(&self, offset: usize) -> Result<u8> {
        Ok(self.bytes(offset, 1)?[0])
    }

    fn i8(&self, offset: usize) -> Result<i8> {
        Ok(self.u8(offset)? as i8)
    }

    fn boolean(&self, offset: usize) -> Result<bool> {
        match self.u8(offset)? {
            0 => Ok(false),
            1 => Ok(true),
            value => bail!("LMU bool at byte {offset} has invalid representation {value}"),
        }
    }

    fn i32(&self, offset: usize) -> Result<i32> {
        let bytes: [u8; 4] = self.bytes(offset, 4)?.try_into()?;
        Ok(i32::from_le_bytes(bytes))
    }

    fn c_string(&self, offset: usize, length: usize) -> Result<String> {
        let bytes = self.bytes(offset, length)?;
        let end = bytes.iter().position(|byte| *byte == 0).unwrap_or(length);
        Ok(String::from_utf8_lossy(&bytes[..end]).trim().to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> Vec<u8> {
        let mut data = vec![0; LMU_SHARED_MEMORY_PAYLOAD_SIZE];
        put_i32(&mut data, GAME_VERSION_OFFSET, 14_200);
        put_i32(&mut data, SCORING_SESSION_OFFSET, 10);
        data
    }

    struct VehicleFixture<'a> {
        driver: &'a str,
        name: &'a str,
        class: &'a str,
        filename: &'a str,
        is_player: bool,
        control: i8,
    }

    fn add_vehicle(data: &mut [u8], index: usize, vehicle: VehicleFixture<'_>) {
        let base = VEHICLE_SCORING_OFFSET + index * VEHICLE_SCORING_SIZE;
        put_text(data, base + VEHICLE_DRIVER_OFFSET, 32, vehicle.driver);
        put_text(data, base + VEHICLE_NAME_OFFSET, 64, vehicle.name);
        put_text(data, base + VEHICLE_CLASS_OFFSET, 32, vehicle.class);
        put_text(data, base + VEHICLE_FILENAME_OFFSET, 32, vehicle.filename);
        data[base + VEHICLE_IS_PLAYER_OFFSET] = u8::from(vehicle.is_player);
        data[base + VEHICLE_CONTROL_OFFSET] = vehicle.control as u8;
    }

    #[test]
    fn decodes_the_marked_player_and_existing_class_alias() {
        let mut data = snapshot();
        put_i32(&mut data, SCORING_NUM_VEHICLES_OFFSET, 2);
        data[SCORING_REALTIME_OFFSET] = 1;
        add_vehicle(
            &mut data,
            0,
            VehicleFixture {
                driver: "Other",
                name: "Porsche 963",
                class: "Hypercar",
                filename: "963.veh",
                is_player: false,
                control: 2,
            },
        );
        add_vehicle(
            &mut data,
            1,
            VehicleFixture {
                driver: "Player",
                name: "Ferrari 296 LMGT3",
                class: "GT3",
                filename: "296.veh",
                is_player: true,
                control: 0,
            },
        );

        let context = decode_lmu_car_context(&data, Some("2026-10-11T21:00:00Z".into())).unwrap();
        assert_eq!(context.vehicle_name.as_deref(), Some("Ferrari 296 LMGT3"));
        assert_eq!(context.game_version, Some(14_200));
        assert_eq!(context.vehicle_id.as_deref(), Some("296.veh"));
        assert_eq!(context.class_raw.as_deref(), Some("GT3"));
        assert_eq!(context.class_normalized.as_deref(), Some("GT3"));
        assert_eq!(context.session_state, LmuSessionState::Realtime);
        assert_eq!(context.status, LmuCarContextStatus::Confirmed);
    }

    #[test]
    fn falls_back_to_one_matching_driver_before_realtime() {
        let mut data = snapshot();
        put_i32(&mut data, SCORING_NUM_VEHICLES_OFFSET, 1);
        put_text(&mut data, SCORING_PLAYER_NAME_OFFSET, 32, "Player");
        add_vehicle(
            &mut data,
            0,
            VehicleFixture {
                driver: "Player",
                name: "Oreca 07",
                class: "LMP2",
                filename: "oreca.veh",
                is_player: false,
                control: -1,
            },
        );

        let context = decode_lmu_car_context(&data, None).unwrap();
        assert_eq!(context.vehicle_name.as_deref(), Some("Oreca 07"));
        assert_eq!(context.session_state, LmuSessionState::Session);
    }

    #[test]
    fn recognizes_replay_control() {
        let mut data = snapshot();
        put_i32(&mut data, SCORING_NUM_VEHICLES_OFFSET, 1);
        add_vehicle(
            &mut data,
            0,
            VehicleFixture {
                driver: "Player",
                name: "Oreca 07",
                class: "LMP2",
                filename: "oreca.veh",
                is_player: true,
                control: 3,
            },
        );

        let context = decode_lmu_car_context(&data, None).unwrap();
        assert_eq!(context.session_state, LmuSessionState::Replay);
    }

    #[test]
    fn preserves_unknown_classes_without_guessing() {
        let mut data = snapshot();
        put_i32(&mut data, SCORING_NUM_VEHICLES_OFFSET, 1);
        add_vehicle(
            &mut data,
            0,
            VehicleFixture {
                driver: "Player",
                name: "Future Car",
                class: "LMH",
                filename: "future.veh",
                is_player: true,
                control: 0,
            },
        );

        let context = decode_lmu_car_context(&data, None).unwrap();
        assert_eq!(context.class_raw.as_deref(), Some("LMH"));
        assert_eq!(context.class_normalized, None);
        assert_eq!(context.status, LmuCarContextStatus::Confirmed);
    }

    #[test]
    fn returns_unknown_when_no_player_can_be_identified() {
        let mut data = snapshot();
        put_i32(&mut data, SCORING_NUM_VEHICLES_OFFSET, 1);
        add_vehicle(
            &mut data,
            0,
            VehicleFixture {
                driver: "Other",
                name: "Porsche 963",
                class: "Hypercar",
                filename: "963.veh",
                is_player: false,
                control: 2,
            },
        );

        let context = decode_lmu_car_context(&data, None).unwrap();
        assert_eq!(context.status, LmuCarContextStatus::Unknown);
        assert_eq!(context.vehicle_name, None);
    }

    #[test]
    fn rejects_incompatible_or_corrupt_snapshots() {
        assert!(decode_lmu_car_context(&[0; 32], None).is_err());

        let mut data = snapshot();
        put_i32(&mut data, SCORING_NUM_VEHICLES_OFFSET, 105);
        assert!(decode_lmu_car_context(&data, None).is_err());

        let mut data = snapshot();
        put_i32(&mut data, SCORING_NUM_VEHICLES_OFFSET, 1);
        data[VEHICLE_SCORING_OFFSET + VEHICLE_IS_PLAYER_OFFSET] = 2;
        assert!(decode_lmu_car_context(&data, None).is_err());
    }

    #[test]
    fn formats_a_readable_utc_observation_time() {
        let time = UNIX_EPOCH + std::time::Duration::from_millis(90_061_123);
        assert_eq!(utc_timestamp(time), "1970-01-02T01:01:01.123Z");
    }

    fn put_i32(data: &mut [u8], offset: usize, value: i32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn put_text(data: &mut [u8], offset: usize, length: usize, value: &str) {
        let bytes = value.as_bytes();
        data[offset..offset + bytes.len().min(length)]
            .copy_from_slice(&bytes[..bytes.len().min(length)]);
    }
}
