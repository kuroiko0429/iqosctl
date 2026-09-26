use std::sync::Arc;

use anyhow::Result;
use iqos::{DeviceStatus, Iqos, IqosBle};
use tokio::sync::Mutex;

use crate::loader::parser::{invalid_arguments, IQOSConsole};

pub fn register_command(console: &mut IQOSConsole) {
    console.register_command(
        "info",
        Box::new(|iqos, args| Box::pin(async move { execute(iqos, args).await })),
    );
}

async fn execute(iqos: Arc<Mutex<Iqos<IqosBle>>>, args: Vec<String>) -> Result<()> {
    let raw = parse_args(&args)?;

    let iqos = iqos.lock().await;
    let model = iqos.transport().model();
    let device_info = iqos.transport().device_info().clone();
    let status = iqos.read_device_status(model, device_info).await?;

    print_status(&status);
    if raw {
        print_raw_frames(&status);
    }
    Ok(())
}

/// Parse `info [--raw]`. Returns whether `--raw` was requested.
fn parse_args(args: &[String]) -> Result<bool> {
    match args.get(1).map(String::as_str) {
        None => Ok(false),
        Some("--raw") if args.len() == 2 => Ok(true),
        _ => Err(invalid_arguments("Usage: info [--raw]")),
    }
}

fn print_status(status: &DeviceStatus) {
    let info = &status.device_info;

    println!("Device Information:");
    println!("  Model:           {:?}", status.model);
    println!(
        "  Model number:    {}",
        field_or_missing(info.model_number.as_deref())
    );
    println!(
        "  Serial number:   {}",
        field_or_missing(info.serial_number.as_deref())
    );
    println!(
        "  Manufacturer:    {}",
        field_or_missing(info.manufacturer_name.as_deref())
    );
    println!(
        "  Software rev:    {}",
        field_or_missing(info.software_revision.as_deref())
    );
    println!("  Product number:  {}", status.product_number);
    println!("  Stick firmware:  {}", status.stick_firmware);

    if let Some(product_number) = &status.holder_product_number {
        println!("  Holder product:  {product_number}");
    }
    if let Some(firmware) = &status.holder_firmware {
        println!("  Holder firmware: {firmware}");
    }

    match status.battery_voltage {
        Some(voltage) => println!("  Battery voltage: {voltage:.3}V"),
        None => println!("  Battery voltage: read failed"),
    }
}

fn field_or_missing(value: Option<&str>) -> &str {
    value.unwrap_or("N/A")
}

/// Print the full raw response frame for every SCP read behind `info`.
///
/// `FirmwareVersion`/`battery_voltage` only decode a handful of bytes out of
/// each frame; on real hardware the rest carries non-zero, currently
/// undecoded data (confirmed via `raw firmware-stick`/`raw battery-voltage`).
/// This surfaces it instead of discarding it.
fn print_raw_frames(status: &DeviceStatus) {
    print_dump("Stick firmware raw", &status.stick_firmware_raw);
    if let Some(raw) = &status.holder_firmware_raw {
        print_dump("Holder firmware raw", raw);
    }
    if let Some(raw) = &status.battery_voltage_raw {
        print_dump("Battery voltage raw", raw);
    }
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
    fn missing_field_falls_back_to_na() {
        assert_eq!(field_or_missing(None), "N/A");
    }

    #[test]
    fn present_field_is_preserved() {
        assert_eq!(field_or_missing(Some("value")), "value");
    }

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    #[test]
    fn parses_no_args_as_non_raw() {
        assert_eq!(parse_args(&args(&["info"])).unwrap(), false);
    }

    #[test]
    fn parses_raw_flag() {
        assert_eq!(parse_args(&args(&["info", "--raw"])).unwrap(), true);
    }

    #[test]
    fn rejects_unknown_flag() {
        assert!(parse_args(&args(&["info", "--bogus"])).is_err());
    }

    #[test]
    fn rejects_trailing_args_after_raw() {
        assert!(parse_args(&args(&["info", "--raw", "extra"])).is_err());
    }
}
