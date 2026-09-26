//! Low-level protocol probe: send a raw SCP command and dump the raw
//! response bytes.
//!
//! Every decoded command in this crate only surfaces the bytes it already
//! knows how to interpret; several response frames are longer than what
//! gets extracted (see e.g. `diagnosis --raw`, which found two undecoded
//! telemetry tags this way). `raw` is the general-purpose tool for
//! inspecting whatever a decoder currently leaves on the table, using
//! exactly the same command bytes the decoded commands send.

use std::sync::Arc;

use anyhow::Result;
use iqos::{Iqos, IqosBle};
use tokio::sync::Mutex;

use crate::loader::parser::{invalid_arguments, IQOSConsole};

pub fn register_command(console: &mut IQOSConsole) {
    console.register_command(
        "raw",
        Box::new(|iqos, args| Box::pin(async move { execute(iqos, args).await })),
    );
}

/// Preset name -> command bytes, using the same constants the decoded
/// commands send. Keep in sync with [`PRESET_NAMES`].
pub(crate) fn preset_command(name: &str) -> Option<&'static [u8]> {
    use iqos::protocol::{
        HOLDER_PRODUCT_NUMBER_COMMAND, LOAD_AUTOSTART_COMMAND, LOAD_BATTERY_VOLTAGE_COMMAND,
        LOAD_BRIGHTNESS_COMMAND, LOAD_FLEXBATTERY_COMMAND, LOAD_FLEXPUFF_COMMAND,
        LOAD_HOLDER_FIRMWARE_VERSION_COMMAND, LOAD_PAUSEMODE_COMMAND,
        LOAD_STICK_FIRMWARE_VERSION_COMMAND, LOAD_TELEMETRY_COMMAND, LOAD_TIMESTAMP_COMMAND,
        LOAD_VIBRATE_CHARGE_START_COMMAND, LOAD_VIBRATION_SETTINGS_COMMAND,
        PRODUCT_NUMBER_COMMAND,
    };

    Some(match name {
        "brightness" => &LOAD_BRIGHTNESS_COMMAND,
        "firmware-stick" => &LOAD_STICK_FIRMWARE_VERSION_COMMAND,
        "firmware-holder" => &LOAD_HOLDER_FIRMWARE_VERSION_COMMAND,
        "autostart" => &LOAD_AUTOSTART_COMMAND,
        "flexpuff" => &LOAD_FLEXPUFF_COMMAND,
        "flexbattery" => &LOAD_FLEXBATTERY_COMMAND,
        "pausemode" => &LOAD_PAUSEMODE_COMMAND,
        "vibration" => &LOAD_VIBRATION_SETTINGS_COMMAND,
        "vibration-charge-start" => &LOAD_VIBRATE_CHARGE_START_COMMAND,
        "battery-voltage" => &LOAD_BATTERY_VOLTAGE_COMMAND,
        "telemetry" => &LOAD_TELEMETRY_COMMAND,
        "timestamp" => &LOAD_TIMESTAMP_COMMAND,
        "product-stick" => &PRODUCT_NUMBER_COMMAND,
        "product-holder" => &HOLDER_PRODUCT_NUMBER_COMMAND,
        _ => return None,
    })
}

/// Preset names accepted by [`preset_command`], for usage text and REPL
/// tab-completion.
pub(crate) const PRESET_NAMES: &[&str] = &[
    "brightness",
    "firmware-stick",
    "firmware-holder",
    "autostart",
    "flexpuff",
    "flexbattery",
    "pausemode",
    "vibration",
    "vibration-charge-start",
    "battery-voltage",
    "telemetry",
    "timestamp",
    "product-stick",
    "product-holder",
];

const USAGE: &str = "Usage: raw <preset|hex-bytes>\n  presets: brightness, firmware-stick, firmware-holder, autostart, flexpuff,\n           flexbattery, pausemode, vibration, vibration-charge-start,\n           battery-voltage, telemetry, timestamp, product-stick, product-holder\n  hex-bytes: e.g. \"00 C0 02 23 C3\" or \"00C00223C3\"";

async fn execute(iqos: Arc<Mutex<Iqos<IqosBle>>>, args: Vec<String>) -> Result<()> {
    if args.len() != 2 {
        return Err(invalid_arguments(USAGE));
    }
    let target = args[1].as_str();
    let command = resolve_command(target)?;

    let iqos = iqos.lock().await;
    let response = iqos.transport().request(&command).await?;

    print_dump("Command", &command);
    print_dump("Response", &response);
    Ok(())
}

fn resolve_command(target: &str) -> Result<Vec<u8>> {
    if let Some(bytes) = preset_command(target) {
        return Ok(bytes.to_vec());
    }
    parse_hex(target)
}

fn parse_hex(value: &str) -> Result<Vec<u8>> {
    let cleaned: String = value.chars().filter(|c| !c.is_whitespace() && *c != ':').collect();
    let invalid = || invalid_arguments(format!("Invalid raw command bytes: {value}\n\n{USAGE}"));

    if cleaned.is_empty()
        || !cleaned.len().is_multiple_of(2)
        || !cleaned.chars().all(|c| c.is_ascii_hexdigit())
    {
        return Err(invalid());
    }

    (0..cleaned.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&cleaned[i..i + 2], 16).map_err(|_| invalid()))
        .collect()
}

fn print_dump(label: &str, bytes: &[u8]) {
    println!("{label} ({} bytes):", bytes.len());
    if bytes.is_empty() {
        println!("  (empty)");
        return;
    }
    for (row, chunk) in bytes.chunks(8).enumerate() {
        let hex: Vec<String> = chunk.iter().map(|byte| format!("{byte:02X}")).collect();
        println!("  [{:>3}] {}", row * 8, hex.join(" "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_spaced_hex() {
        assert_eq!(parse_hex("00 C0 02 23 C3").unwrap(), vec![0x00, 0xC0, 0x02, 0x23, 0xC3]);
    }

    #[test]
    fn parses_unspaced_hex() {
        assert_eq!(parse_hex("00C00223C3").unwrap(), vec![0x00, 0xC0, 0x02, 0x23, 0xC3]);
    }

    #[test]
    fn parses_colon_separated_hex() {
        assert_eq!(parse_hex("00:C0:02:23:C3").unwrap(), vec![0x00, 0xC0, 0x02, 0x23, 0xC3]);
    }

    #[test]
    fn rejects_odd_length_hex() {
        assert!(parse_hex("0C0").is_err());
    }

    #[test]
    fn rejects_non_hex_characters() {
        assert!(parse_hex("ZZ").is_err());
    }

    #[test]
    fn rejects_empty_input() {
        assert!(parse_hex("").is_err());
    }

    #[test]
    fn resolves_known_presets() {
        for name in PRESET_NAMES {
            assert!(preset_command(name).is_some(), "missing preset: {name}");
        }
    }

    #[test]
    fn unknown_preset_falls_back_to_hex_parsing() {
        assert_eq!(resolve_command("00C0022 3C3".replace(' ', "").as_str()).unwrap().len(), 5);
    }
}
