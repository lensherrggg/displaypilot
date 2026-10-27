use chrono::{SecondsFormat, Utc};
use serde::Serialize;

/// Stable, deterministic display identifier derived from EDID-like fields.
/// Enumeration order and OS-specific paths are only used when no serial exists.
pub fn stable_display_id(
    vendor_id: Option<&str>,
    model_id: Option<u16>,
    model_name: Option<&str>,
    serial_number: Option<&str>,
    edid_serial: Option<u32>,
    backend: &str,
    native_id: &str,
) -> String {
    let vendor = vendor_id.unwrap_or("").trim().to_uppercase();
    let model_name = model_name.unwrap_or("").trim().to_uppercase();
    let serial_text = serial_number.map(str::trim).unwrap_or("");

    let seed = if !serial_text.is_empty() {
        format!("{vendor}|{model_id:?}|{model_name}|sn:{serial_text}")
    } else if let Some(serial) = edid_serial.filter(|s| *s != 0) {
        format!("{vendor}|{model_id:?}|{model_name}|edid:{serial}")
    } else {
        // Last resort. The backend path can change across reconnects, but a device
        // with no serial number cannot be identified more strongly than this.
        format!("{vendor}|{model_id:?}|{model_name}|{backend}|{native_id}")
    };

    format!("dsp-{:016x}", fnv1a64(seed.as_bytes()))
}

fn fnv1a64(data: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

/// Case-insensitive glob matching supporting `*` and `?`.
pub fn wildcard_match(pattern: &str, value: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let value: Vec<char> = value.to_lowercase().chars().collect();
    wildcard_inner(&pattern, &value)
}

fn wildcard_inner(pattern: &[char], value: &[char]) -> bool {
    let mut p = 0;
    let mut v = 0;
    let mut star = None;
    let mut star_value = 0;

    while v < value.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == value[v]) {
            p += 1;
            v += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            star_value = v;
            p += 1;
        } else if let Some(star_pos) = star {
            p = star_pos + 1;
            star_value += 1;
            v = star_value;
        } else {
            return false;
        }
    }

    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

pub fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn clamp_percent(value: u8) -> u8 {
    value.min(100)
}

/// Maps a 0–100 UI percentage onto a VCP range using rounded linear scaling.
pub fn percent_to_raw(percent: u8, min: u16, max: u16) -> u16 {
    if max <= min {
        return min;
    }
    let percent = u32::from(percent.min(100));
    let span = u32::from(max - min);
    min + ((span * percent + 50) / 100) as u16
}

pub fn raw_to_percent(raw: u16, min: u16, max: u16) -> u8 {
    if max <= min {
        return 0;
    }
    let raw = raw.clamp(min, max);
    let numerator = u32::from(raw - min) * 100;
    let denominator = u32::from(max - min);
    ((numerator + denominator / 2) / denominator) as u8
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub os: &'static str,
    pub arch: &'static str,
}

pub fn platform_info() -> PlatformInfo {
    PlatformInfo {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_id_uses_serial_instead_of_order() {
        let a = stable_display_id(
            Some("GSM"),
            Some(1),
            Some("X"),
            Some("SN1"),
            None,
            "winapi",
            "path-a",
        );
        let b = stable_display_id(
            Some("gsm"),
            Some(1),
            Some("x"),
            Some("SN1"),
            None,
            "winapi",
            "path-b",
        );
        assert_eq!(a, b);
        assert!(a.starts_with("dsp-"));
    }

    #[test]
    fn stable_id_falls_back_to_native_path_without_serial() {
        let a = stable_display_id(
            Some("DEL"),
            Some(2),
            Some("U"),
            None,
            None,
            "winapi",
            "path-a",
        );
        let b = stable_display_id(
            Some("DEL"),
            Some(2),
            Some("U"),
            None,
            None,
            "winapi",
            "path-b",
        );
        assert_ne!(a, b);
    }

    #[test]
    fn wildcard_matching_is_case_insensitive() {
        assert!(wildcard_match("*27ul850*", "LG 27UL850-W"));
        assert!(wildcard_match("gsm????", "GSM5A3F"));
        assert!(!wildcard_match("*dell*", "LG 27UL850"));
        assert!(wildcard_match("*", ""));
    }

    #[test]
    fn percent_mapping_round_trips() {
        assert_eq!(percent_to_raw(0, 0, 255), 0);
        assert_eq!(percent_to_raw(100, 0, 255), 255);
        assert_eq!(percent_to_raw(50, 0, 255), 128);
        assert_eq!(raw_to_percent(128, 0, 255), 50);
        assert_eq!(percent_to_raw(60, 10, 20), 16);
        assert_eq!(raw_to_percent(15, 10, 20), 50);
    }

    #[test]
    fn percent_mapping_clamps() {
        assert_eq!(percent_to_raw(120, 0, 100), 100);
        assert_eq!(raw_to_percent(300, 0, 100), 100);
        assert_eq!(percent_to_raw(50, 7, 7), 7);
        assert_eq!(raw_to_percent(7, 7, 7), 0);
    }
}
