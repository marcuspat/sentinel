use ed25519_dalek::VerifyingKey;
use sentinel_audit::{AuditVerifier, Checkpoint};
use sentinel_sysadmin_core::{TimestampPrecision, UtcTimestamp, UtcWeekday};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
struct LegacyFixture {
    timestamp_cases: Vec<TimestampCase>,
    log_lines: Vec<String>,
    checkpoints: Vec<SignedCheckpoint>,
    weekdays: Vec<WeekdayCase>,
}

#[derive(Deserialize)]
struct TimestampCase {
    input: String,
    wire: Value,
    millis: String,
    nanos: String,
}

#[derive(Deserialize)]
struct SignedCheckpoint {
    wire: String,
    message: String,
}

#[derive(Deserialize)]
struct WeekdayCase {
    input: String,
    wire: Value,
}

// Produced by Sentinel v0.2.1 (c1cb769b361c) with fixed IDs and a synthetic signing key.
fn legacy_fixture() -> LegacyFixture {
    serde_json::from_str(include_str!("fixtures/v0.2.1-timestamps.json")).unwrap()
}

#[test]
fn legacy_timestamps_and_weekdays_round_trip() {
    let fixture = legacy_fixture();
    for case in fixture.timestamp_cases {
        let timestamp: UtcTimestamp = serde_json::from_value(case.input.clone().into()).unwrap();
        assert_eq!(
            serde_json::to_value(timestamp).unwrap(),
            case.wire,
            "{}",
            case.input
        );
        assert_eq!(
            timestamp
                .rfc3339(TimestampPrecision::Milliseconds)
                .to_string(),
            case.millis
        );
        assert_eq!(
            timestamp
                .rfc3339(TimestampPrecision::Nanoseconds)
                .to_string(),
            case.nanos
        );
    }
    for case in fixture.weekdays {
        let weekday: UtcWeekday = serde_json::from_value(case.input.clone().into()).unwrap();
        assert_eq!(
            serde_json::to_value(weekday).unwrap(),
            case.wire,
            "{}",
            case.input
        );
    }
}

#[test]
fn legacy_audit_chain_verifies_and_keeps_its_jsonl_bytes() {
    let fixture = legacy_fixture();
    let jsonl = fixture.log_lines.join("\n");
    let verification = AuditVerifier::verify_jsonl(&jsonl).unwrap();
    assert!(verification.valid, "{verification:?}");
    assert_eq!(verification.events_checked, fixture.log_lines.len());

    let events = AuditVerifier::parse_jsonl(&jsonl).unwrap();
    let encoded: Vec<_> = events
        .iter()
        .map(|event| serde_json::to_string(event).unwrap())
        .collect();
    assert_eq!(encoded, fixture.log_lines);
}

#[test]
fn legacy_ed25519_checkpoints_keep_their_signed_bytes() {
    for case in legacy_fixture().checkpoints {
        let checkpoint: Checkpoint = serde_json::from_str(&case.wire).unwrap();
        let public: [u8; 32] = hex::decode(&checkpoint.public_key)
            .unwrap()
            .try_into()
            .unwrap();
        let key = VerifyingKey::from_bytes(&public).unwrap();

        assert_eq!(
            Checkpoint::signing_message(
                checkpoint.session_id,
                checkpoint.event_count,
                &checkpoint.head_hash,
                &checkpoint.signed_at,
            ),
            case.message.as_bytes(),
        );
        assert!(checkpoint.verify_signature(&key));
        assert_eq!(serde_json::to_string(&checkpoint).unwrap(), case.wire);
    }
}
