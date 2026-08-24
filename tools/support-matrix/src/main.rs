//! Verify the machine-readable Matter 1.6 device support matrix.

use std::collections::BTreeSet;
use std::process::ExitCode;

use serde_json::Value;

const EXPECTED_IDS: [u32; 91] = [
    0x000A, 0x000B, 0x000E, 0x000F, 0x0011, 0x0012, 0x0013, 0x0014, 0x0015, 0x0016,
    0x0017, 0x0018, 0x0019, 0x0022, 0x0023, 0x0024, 0x0027, 0x0028, 0x0029, 0x002A,
    0x002B, 0x002C, 0x002D, 0x0040, 0x0041, 0x0042, 0x0043, 0x0044, 0x0045, 0x0070,
    0x0071, 0x0072, 0x0073, 0x0074, 0x0075, 0x0076, 0x0077, 0x0078, 0x0079, 0x007A,
    0x007B, 0x007C, 0x0090, 0x0091, 0x0100, 0x0101, 0x0103, 0x0104, 0x0105, 0x0106,
    0x0107, 0x010A, 0x010B, 0x010C, 0x010D, 0x010F, 0x0110, 0x0130, 0x0140, 0x0141,
    0x0142, 0x0143, 0x0144, 0x0145, 0x0146, 0x0147, 0x0148, 0x0202, 0x0203, 0x0230,
    0x0231, 0x023E, 0x0301, 0x0302, 0x0303, 0x0304, 0x0305, 0x0306, 0x0307, 0x0309,
    0x030A, 0x050C, 0x050D, 0x050F, 0x0510, 0x0511, 0x0512, 0x0513, 0x0514, 0x0840,
    0x0850,
];

const EXCLUDED_IDS: [u32; 3] = [0x0074, 0x0142, 0x0143];
const EXCLUDED_NAMES: [&str; 3] = [
    "Robotic Vacuum Cleaner",
    "Camera",
    "Video Doorbell",
];

fn main() -> ExitCode {
    match verify(include_str!("../../../device-support.json")) {
        Ok(summary) => {
            println!(
                "Matter {}: {} implemented, {} excluded, {} total",
                summary.version, summary.implemented, summary.excluded, summary.total
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("device support matrix error: {error}");
            ExitCode::FAILURE
        }
    }
}

struct Summary<'a> {
    version: &'a str,
    implemented: usize,
    excluded: usize,
    total: usize,
}

fn verify(input: &str) -> Result<Summary<'_>, String> {
    let root: Value = serde_json::from_str(input).map_err(|error| error.to_string())?;
    let version = root
        .get("matter_version")
        .and_then(Value::as_str)
        .ok_or_else(|| String::from("missing matter_version"))?;
    if version != "1.6" {
        return Err(format!("expected Matter 1.6, found {version}"));
    }

    let devices = root
        .get("device_types")
        .and_then(Value::as_array)
        .ok_or_else(|| String::from("missing device_types array"))?;
    if devices.len() != EXPECTED_IDS.len() {
        return Err(format!(
            "expected {} device types, found {}",
            EXPECTED_IDS.len(),
            devices.len()
        ));
    }

    let expected: BTreeSet<u32> = EXPECTED_IDS.into_iter().collect();
    let mut seen = BTreeSet::new();
    let mut implemented = 0;
    let mut excluded = 0;
    let mut excluded_seen = BTreeSet::new();

    for device in devices {
        let id = device
            .get("id")
            .and_then(Value::as_u64)
            .and_then(|value| u32::try_from(value).ok())
            .ok_or_else(|| String::from("device has invalid id"))?;
        let name = required_string(device, "name", id)?;
        if !expected.contains(&id) {
            return Err(format!("unexpected device id 0x{id:04X}: {name}"));
        }
        if !seen.insert(id) {
            return Err(format!("duplicate device id 0x{id:04X}"));
        }

        let status = required_string(device, "status", id)?;
        if EXCLUDED_IDS.contains(&id) {
            excluded += 1;
            excluded_seen.insert(id);
            if status != "excluded" {
                return Err(format!("excluded device 0x{id:04X} is marked {status}"));
            }
            if !EXCLUDED_NAMES.contains(&name) {
                return Err(format!("unexpected exclusion name for 0x{id:04X}: {name}"));
            }
            let reason = required_string(device, "exclusion_reason", id)?;
            if reason != "explicit-user-exclusion" {
                return Err(format!("invalid exclusion reason for 0x{id:04X}"));
            }
        } else {
            implemented += 1;
            if status != "implemented" {
                return Err(format!("device 0x{id:04X} is not implemented"));
            }
            for field in ["feature", "typed_api", "protocol_handler", "test"] {
                let value = required_string(device, field, id)?;
                if value.trim().is_empty() {
                    return Err(format!("empty {field} for 0x{id:04X}"));
                }
            }
            let clusters = device
                .get("clusters")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("missing clusters for 0x{id:04X}"))?;
            if clusters.is_empty() {
                return Err(format!("empty cluster list for 0x{id:04X}"));
            }
        }
    }

    if seen != expected {
        return Err(String::from("matrix does not match the Matter 1.6 ID set"));
    }
    let expected_exclusions: BTreeSet<u32> = EXCLUDED_IDS.into_iter().collect();
    if excluded_seen != expected_exclusions || excluded != 3 || implemented != 88 {
        return Err(format!(
            "expected 88 implemented and 3 excluded, found {implemented} and {excluded}"
        ));
    }

    Ok(Summary {
        version,
        implemented,
        excluded,
        total: devices.len(),
    })
}

fn required_string<'a>(device: &'a Value, field: &str, id: u32) -> Result<&'a str, String> {
    device
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing {field} for 0x{id:04X}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_in_matrix_is_complete() {
        assert!(verify(include_str!("../../../device-support.json")).is_ok());
    }
}
