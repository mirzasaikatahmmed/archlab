//! Keeps Rust validation ranges in sync with the zod schemas in the UI.
//! Both sides test against the shared fixture `src/schemas/limits.json`.

use serde::Deserialize;
use serde_json::{Value, json};

use super::server_profile::ServerProfile;
use super::traffic::TrafficProfile;

#[derive(Deserialize)]
struct Limit {
    path: String,
    min: f64,
    max: f64,
    below: f64,
    above: f64,
}

#[derive(Deserialize)]
struct Limits {
    #[serde(rename = "serverProfile")]
    server_profile: Vec<Limit>,
    #[serde(rename = "trafficProfile")]
    traffic_profile: Vec<Limit>,
}

fn load() -> Limits {
    let raw = include_str!("../../../src/schemas/limits.json");
    serde_json::from_str(raw).expect("limits.json parses")
}

fn set_path(root: &mut Value, path: &str, value: f64) {
    let mut cur = root;
    let mut parts = path.split('.').peekable();
    while let Some(part) = parts.next() {
        if parts.peek().is_none() {
            cur[part] = if value.fract() == 0.0 {
                json!(value as i64)
            } else {
                json!(value)
            };
        } else {
            cur = &mut cur[part];
        }
    }
}

fn is_valid<T>(base: &T, path: &str, value: f64, validate: impl Fn(&T) -> bool) -> bool
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let mut v = serde_json::to_value(base).expect("serialize");
    set_path(&mut v, path, value);
    serde_json::from_value::<T>(v).is_ok_and(|t| validate(&t))
}

fn check<T>(base: &T, limits: &[Limit], validate: impl Fn(&T) -> bool + Copy)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    for l in limits {
        assert!(is_valid(base, &l.path, l.min, validate), "{} min", l.path);
        assert!(is_valid(base, &l.path, l.max, validate), "{} max", l.path);
        // Out-of-range integers may fail to deserialize (e.g. negative u32) — both are rejections.
        assert!(
            !is_valid(base, &l.path, l.below, validate),
            "{} below",
            l.path
        );
        assert!(
            !is_valid(base, &l.path, l.above, validate),
            "{} above",
            l.path
        );
        assert!(!l.path.is_empty() && l.min <= l.max);
    }
}

#[test]
fn server_profile_ranges_match_shared_fixture() {
    check(&ServerProfile::default(), &load().server_profile, |p| {
        p.validate().is_ok()
    });
}

#[test]
fn traffic_profile_ranges_match_shared_fixture() {
    check(&TrafficProfile::default(), &load().traffic_profile, |p| {
        p.validate().is_ok()
    });
}
