//! JSON representations for data commands (`battery`, `info`, `diagnosis`).
//!
//! Field sets intentionally mirror the text output in
//! `loader::cmds::{battery,info,diagnosis}` so scripts consuming `--format
//! json` see the same data a human sees in text mode.

use iqos::{
    DeviceStatus, DiagnosticData, TELEMETRY_TAG_DAY_COUNTER, TELEMETRY_TAG_PUFF_COUNT,
};
use serde_json::{json, Value};

/// JSON body for the `battery` command.
pub fn battery(level: u8) -> Value {
    json!({ "battery_percent": level })
}

/// JSON body for the `diagnosis` command.
///
/// Always includes the full `telemetry_tags` list (not just recognized
/// ones) since JSON is machine-consumed; there is no `--raw` toggle here,
/// unlike the text output.
pub fn diagnosis(data: &DiagnosticData) -> Value {
    let telemetry_tags: Vec<Value> = data
        .telemetry_tags
        .iter()
        .map(|entry| {
            json!({
                "tag": format!("0x{:02X}", entry.tag),
                "value": entry.value,
                "known_as": telemetry_tag_label(entry.tag),
            })
        })
        .collect();

    json!({
        "total_puffs": data.total_smoking_count,
        "days_used": data.days_used,
        "battery_voltage": data.battery_voltage,
        "telemetry_tags": telemetry_tags,
    })
}

fn telemetry_tag_label(tag: u8) -> Option<&'static str> {
    match tag {
        TELEMETRY_TAG_PUFF_COUNT => Some("total_smoking_count"),
        TELEMETRY_TAG_DAY_COUNTER => Some("days_used"),
        _ => None,
    }
}

/// JSON body for the `info` command.
///
/// Always includes the raw firmware/battery-voltage response frames as hex
/// strings (not just the decoded fields), same rationale as `diagnosis`'s
/// `telemetry_tags`: JSON is machine-consumed, so there's no reason to hide
/// data behind a `--raw` toggle the way the text output does.
pub fn device_status(status: &DeviceStatus) -> Value {
    let info = &status.device_info;
    json!({
        "model": format!("{:?}", status.model),
        "model_number": info.model_number,
        "serial_number": info.serial_number,
        "manufacturer": info.manufacturer_name,
        "software_revision": info.software_revision,
        "product_number": status.product_number,
        "stick_firmware": status.stick_firmware.to_string(),
        "stick_firmware_raw": hex_string(&status.stick_firmware_raw),
        "holder_product_number": status.holder_product_number,
        "holder_firmware": status.holder_firmware.map(|firmware| firmware.to_string()),
        "holder_firmware_raw": status.holder_firmware_raw.as_deref().map(hex_string),
        "battery_voltage": status.battery_voltage,
        "battery_voltage_raw": status.battery_voltage_raw.as_deref().map(hex_string),
    })
}

fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect::<Vec<_>>().join(" ")
}
