//! JSON representations for data commands (`battery`, `info`, `diagnosis`).
//!
//! Field sets intentionally mirror the text output in
//! `loader::cmds::{battery,info,diagnosis}` so scripts consuming `--format
//! json` see the same data a human sees in text mode.

use iqos::{DeviceStatus, DiagnosticData};
use serde_json::{json, Value};

/// JSON body for the `battery` command.
pub fn battery(level: u8) -> Value {
    json!({ "battery_percent": level })
}

/// JSON body for the `diagnosis` command.
pub fn diagnosis(data: &DiagnosticData) -> Value {
    json!({
        "total_puffs": data.total_smoking_count,
        "days_used": data.days_used,
        "battery_voltage": data.battery_voltage,
    })
}

/// JSON body for the `info` command.
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
        "holder_product_number": status.holder_product_number,
        "holder_firmware": status.holder_firmware.map(|firmware| firmware.to_string()),
        "battery_voltage": status.battery_voltage,
    })
}
