use std::sync::Arc;

use anyhow::Result;
use iqos::{DiagnosticData, Iqos, IqosBle, TELEMETRY_TAG_DAY_COUNTER, TELEMETRY_TAG_PUFF_COUNT};
use tokio::sync::Mutex;

use crate::loader::parser::{invalid_arguments, IQOSConsole};

pub fn register_command(console: &mut IQOSConsole) {
    console.register_command(
        "diagnosis",
        Box::new(|iqos, args| Box::pin(async move { execute(iqos, args).await })),
    );
}

async fn execute(iqos: Arc<Mutex<Iqos<IqosBle>>>, args: Vec<String>) -> Result<()> {
    let raw = parse_args(&args)?;

    let iqos = iqos.lock().await;
    let data = iqos.read_diagnosis().await?;

    print_summary(&data);
    if raw {
        print_raw_tags(&data);
    }
    Ok(())
}

/// Parse `diagnosis [--raw]`. Returns whether `--raw` was requested.
fn parse_args(args: &[String]) -> Result<bool> {
    match args.get(1).map(String::as_str) {
        None => Ok(false),
        Some("--raw") if args.len() == 2 => Ok(true),
        _ => Err(invalid_arguments("Usage: diagnosis [--raw]")),
    }
}

fn print_summary(data: &DiagnosticData) {
    println!("Diagnosis:");
    if let Some(count) = data.total_smoking_count {
        println!("  Total puffs:     {count}");
    }
    if let Some(days) = data.days_used {
        println!("  Days used:       {days}");
    }
    if let Some(volts) = data.battery_voltage {
        println!("  Battery voltage: {volts:.2}V");
    }
}

/// Print every telemetry tag/value block the device reported, including ones
/// this crate has no named field for. Real devices report more tags per
/// frame than are currently decoded (see `iqos::TelemetryTag`); this
/// surfaces the rest instead of silently discarding them.
fn print_raw_tags(data: &DiagnosticData) {
    println!("Raw telemetry tags ({} total):", data.telemetry_tags.len());
    if data.telemetry_tags.is_empty() {
        println!("  (none)");
        return;
    }
    for entry in &data.telemetry_tags {
        match tag_label(entry.tag) {
            Some(label) => {
                println!("  tag=0x{:02X} value={:<6} ({label})", entry.tag, entry.value);
            }
            None => {
                println!("  tag=0x{:02X} value={:<6} (unrecognized)", entry.tag, entry.value);
            }
        }
    }
}

fn tag_label(tag: u8) -> Option<&'static str> {
    match tag {
        TELEMETRY_TAG_PUFF_COUNT => Some("total_smoking_count"),
        TELEMETRY_TAG_DAY_COUNTER => Some("days_used"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| (*part).to_owned()).collect()
    }

    #[test]
    fn parses_no_args_as_non_raw() {
        assert_eq!(parse_args(&args(&["diagnosis"])).unwrap(), false);
    }

    #[test]
    fn parses_raw_flag() {
        assert_eq!(parse_args(&args(&["diagnosis", "--raw"])).unwrap(), true);
    }

    #[test]
    fn rejects_unknown_flag() {
        assert!(parse_args(&args(&["diagnosis", "--bogus"])).is_err());
    }

    #[test]
    fn rejects_trailing_args_after_raw() {
        assert!(parse_args(&args(&["diagnosis", "--raw", "extra"])).is_err());
    }

    #[test]
    fn labels_known_tags() {
        assert_eq!(tag_label(TELEMETRY_TAG_PUFF_COUNT), Some("total_smoking_count"));
        assert_eq!(tag_label(TELEMETRY_TAG_DAY_COUNTER), Some("days_used"));
    }

    #[test]
    fn unknown_tag_has_no_label() {
        assert_eq!(tag_label(0x33), None);
    }
}
