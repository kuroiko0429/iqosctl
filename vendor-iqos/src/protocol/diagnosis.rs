use crate::{Error, Result};

/// Command used to request telemetry data (puff count, days used).
pub const LOAD_TELEMETRY_COMMAND: [u8; 8] = [0x00, 0xC9, 0x10, 0x02, 0x01, 0x01, 0x75, 0xD6];

/// Command used to request timestamp data (days used).
pub const LOAD_TIMESTAMP_COMMAND: [u8; 8] = [0x00, 0xC0, 0x10, 0x02, 0x00, 0x04, 0x38, 0xEF];

/// Command used to request battery voltage.
pub const LOAD_BATTERY_VOLTAGE_COMMAND: [u8; 5] = [0x00, 0xC0, 0x00, 0x21, 0xE7];

/// All commands sent during a full diagnostic read, in order.
///
/// The telemetry command is sent twice to match the reference implementation's
/// observed sequence.
pub const ALL_DIAGNOSIS_COMMANDS: [&[u8]; 4] = [
    &LOAD_TELEMETRY_COMMAND,
    &LOAD_TIMESTAMP_COMMAND,
    &LOAD_TELEMETRY_COMMAND,
    &LOAD_BATTERY_VOLTAGE_COMMAND,
];

// Response header discriminants at bytes[2..4].
const TELEMETRY_HEADER: [u8; 2] = [0x90, 0x22];
const TIMESTAMP_HEADER: [u8; 2] = [0x80, 0x02];
const BATTERY_VOLTAGE_HEADER: [u8; 2] = [0x88, 0x21];

/// Tag identifier for the puff-count block within a telemetry frame.
pub const TELEMETRY_TAG_PUFF_COUNT: u8 = 0x8E;
/// Tag identifier for the day-counter block within a telemetry frame.
pub const TELEMETRY_TAG_DAY_COUNTER: u8 = 0x17;

/// A single tag/value block parsed from a telemetry response frame.
///
/// Every block the device reports is captured here, whether or not this
/// crate has a named field for it — see [`DiagnosticData::telemetry_tags`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TelemetryTag {
    /// Tag byte identifying the kind of value in this block.
    pub tag: u8,
    /// Raw 16-bit value associated with the tag.
    pub value: u16,
}

/// Diagnostic telemetry data collected from the device.
///
/// The named fields are optional because each is sourced from a separate
/// response frame and remains `None` if that frame could not be parsed.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct DiagnosticData {
    /// Total puff (smoking) count.
    pub total_smoking_count: Option<u16>,
    /// Number of days the device has been used.
    pub days_used: Option<u16>,
    /// Current battery voltage in volts.
    pub battery_voltage: Option<f32>,
    /// Every tag/value block seen across all telemetry frames, in the order
    /// received, including tags this crate doesn't map to a named field
    /// above (e.g. via [`TELEMETRY_TAG_PUFF_COUNT`]/[`TELEMETRY_TAG_DAY_COUNTER`]).
    /// Useful for inspecting device telemetry this crate doesn't decode yet.
    pub telemetry_tags: Vec<TelemetryTag>,
}

impl DiagnosticData {
    /// Create an empty `DiagnosticData`.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// An 8-byte block parsed from a telemetry response frame.
struct TelemetryBlock {
    value: u16,
    tag: u8,
}

impl TelemetryBlock {
    const SIZE: usize = 8;

    fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < Self::SIZE {
            return Err(Error::ProtocolDecode("invalid telemetry block: too short".to_string()));
        }
        Ok(Self { value: u16::from_le_bytes([bytes[4], bytes[5]]), tag: bytes[7] })
    }
}

/// Incrementally builds a [`DiagnosticData`] by parsing successive response
/// frames.
///
/// Each call to [`parse`](Self::parse) auto-detects the response type from the
/// header bytes and populates the corresponding fields.
#[derive(Debug, Default, Clone)]
pub(crate) struct DiagnosticDataBuilder {
    inner: DiagnosticData,
}

impl DiagnosticDataBuilder {
    /// Parse a single response frame and update internal state.
    ///
    /// Unknown headers are silently ignored so that unrecognised frames do not
    /// abort the collection.
    ///
    /// # Errors
    ///
    /// Returns an error if the frame is too short to extract a 2-byte header,
    /// or if a recognised frame's payload cannot be decoded.
    pub fn parse(mut self, bytes: &[u8]) -> Result<Self> {
        if bytes.len() < 4 {
            return Err(Error::ProtocolDecode(
                "diagnosis response too short for header".to_string(),
            ));
        }

        match [bytes[2], bytes[3]] {
            TELEMETRY_HEADER => self.parse_telemetry(bytes)?,
            TIMESTAMP_HEADER => self.parse_timestamp(bytes)?,
            BATTERY_VOLTAGE_HEADER => self.parse_battery_voltage(bytes)?,
            _ => {} // unknown frame — skip gracefully
        }

        Ok(self)
    }

    /// Finalise and return the accumulated [`DiagnosticData`].
    #[must_use]
    pub fn build(self) -> DiagnosticData {
        self.inner
    }

    fn parse_telemetry(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() < 6 {
            return Err(Error::ProtocolDecode(
                "invalid telemetry frame: too short for marker".to_string(),
            ));
        }

        let marker = u16::from_le_bytes([bytes[4], bytes[5]]);
        if marker != 0x0101 {
            return Err(Error::ProtocolDecode("invalid telemetry frame: wrong marker".to_string()));
        }

        let length_byte = bytes[3] as usize;
        let num_blocks = length_byte.saturating_sub(2) / TelemetryBlock::SIZE;
        let blocks_start: usize = 6;
        let required = blocks_start + num_blocks * TelemetryBlock::SIZE;

        if bytes.len() < required {
            return Err(Error::ProtocolDecode(format!(
                "invalid telemetry frame: expected {required} bytes, got {}",
                bytes.len()
            )));
        }

        for i in 0..num_blocks {
            let offset = blocks_start + i * TelemetryBlock::SIZE;
            let block = TelemetryBlock::parse(&bytes[offset..offset + TelemetryBlock::SIZE])?;
            self.inner.telemetry_tags.push(TelemetryTag { tag: block.tag, value: block.value });
            match block.tag {
                TELEMETRY_TAG_PUFF_COUNT => self.inner.total_smoking_count = Some(block.value),
                TELEMETRY_TAG_DAY_COUNTER => self.inner.days_used = Some(block.value),
                _ => {}
            }
        }

        Ok(())
    }

    fn parse_timestamp(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() < 6 {
            return Err(Error::ProtocolDecode("invalid timestamp frame: too short".to_string()));
        }
        self.inner.days_used = Some(u16::from_le_bytes([bytes[4], bytes[5]]));
        Ok(())
    }

    fn parse_battery_voltage(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.len() < 7 {
            return Err(Error::ProtocolDecode(
                "invalid battery voltage frame: too short".to_string(),
            ));
        }
        let raw = u16::from_le_bytes([bytes[5], bytes[6]]);
        self.inner.battery_voltage = Some(f32::from(raw) / 1000.0);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ALL_DIAGNOSIS_COMMANDS, DiagnosticData, DiagnosticDataBuilder, LOAD_BATTERY_VOLTAGE_COMMAND,
        LOAD_TELEMETRY_COMMAND, LOAD_TIMESTAMP_COMMAND, TelemetryTag,
    };

    #[test]
    fn keeps_load_commands_stable() {
        assert_eq!(LOAD_TELEMETRY_COMMAND, [0x00, 0xC9, 0x10, 0x02, 0x01, 0x01, 0x75, 0xD6]);
        assert_eq!(LOAD_TIMESTAMP_COMMAND, [0x00, 0xC0, 0x10, 0x02, 0x00, 0x04, 0x38, 0xEF]);
        assert_eq!(LOAD_BATTERY_VOLTAGE_COMMAND, [0x00, 0xC0, 0x00, 0x21, 0xE7]);
    }

    #[test]
    fn diagnosis_commands_contains_four_entries() {
        assert_eq!(ALL_DIAGNOSIS_COMMANDS.len(), 4);
    }

    #[test]
    fn parses_battery_voltage_frame() {
        // header [0x88, 0x21] at bytes[2..4], raw voltage at bytes[5..7]
        // 4200 mV = 4.2 V
        let bytes = [0x00, 0x08, 0x88, 0x21, 0x00, 0xA8, 0x10, 0x00, 0x00];
        let result = DiagnosticDataBuilder::default().parse(&bytes).unwrap().build();
        // bytes[5] = 0xA8, bytes[6] = 0x10 → u16 LE = 0x10A8 = 4264
        assert_eq!(result.battery_voltage, Some(4264_f32 / 1000.0));
    }

    #[test]
    fn parses_timestamp_frame() {
        // header [0x80, 0x02] at bytes[2..4], days at bytes[4..6]
        let bytes = [0x00, 0x08, 0x80, 0x02, 0x1E, 0x00, 0x00, 0x00];
        let result = DiagnosticDataBuilder::default().parse(&bytes).unwrap().build();
        assert_eq!(result.days_used, Some(0x001E)); // 30 days
    }

    #[test]
    fn ignores_unknown_header_frames() {
        let bytes = [0x00, 0x08, 0xFF, 0xFF, 0x00, 0x00, 0x00, 0x00];
        let result = DiagnosticDataBuilder::default().parse(&bytes).unwrap().build();
        assert_eq!(result, DiagnosticData::default());
    }

    #[test]
    fn rejects_too_short_frame() {
        let error = DiagnosticDataBuilder::default().parse(&[0x00, 0x08, 0x88]);
        assert!(error.is_err());
    }

    #[test]
    fn rejects_too_short_battery_voltage_frame() {
        // header matches but not enough bytes for voltage
        let bytes = [0x00, 0x08, 0x88, 0x21, 0x00, 0xA8];
        let error = DiagnosticDataBuilder::default().parse(&bytes);
        assert!(error.is_err());
    }

    #[test]
    fn rejects_too_short_timestamp_frame() {
        let bytes = [0x00, 0x08, 0x80, 0x02, 0x1E];
        let error = DiagnosticDataBuilder::default().parse(&bytes);
        assert!(error.is_err());
    }

    #[test]
    fn builder_accumulates_across_multiple_frames() {
        let timestamp_bytes = [0x00, 0x08, 0x80, 0x02, 0x0A, 0x00, 0x00, 0x00];
        let battery_bytes = [0x00, 0x08, 0x88, 0x21, 0x00, 0xE8, 0x0F, 0x00, 0x00];
        let result = DiagnosticDataBuilder::default()
            .parse(&timestamp_bytes)
            .unwrap()
            .parse(&battery_bytes)
            .unwrap()
            .build();
        assert_eq!(result.days_used, Some(10));
        // bytes[5]=0xE8, bytes[6]=0x0F → 0x0FE8 = 4072
        assert_eq!(result.battery_voltage, Some(4072_f32 / 1000.0));
        assert_eq!(result.total_smoking_count, None);
    }

    #[test]
    fn telemetry_tags_capture_unrecognized_tags_alongside_known_ones() {
        // header [0x90, 0x22] forces length_byte=0x22=34 -> num_blocks=4,
        // matching what the device actually sends: 4 tag blocks per
        // telemetry frame, only 2 of which (0x8E, 0x17) get a named field.
        #[rustfmt::skip]
        let bytes: [u8; 38] = [
            0x00, 0x00, 0x90, 0x22, 0x01, 0x01,
            // block 0: tag 0x8E (known: total_smoking_count), value 1234
            0x00, 0x00, 0x00, 0x00, 0xD2, 0x04, 0x00, 0x8E,
            // block 1: tag 0x17 (known: days_used), value 42
            0x00, 0x00, 0x00, 0x00, 0x2A, 0x00, 0x00, 0x17,
            // block 2: tag 0x33 (unrecognized), value 7
            0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x33,
            // block 3: tag 0x99 (unrecognized), value 999
            0x00, 0x00, 0x00, 0x00, 0xE7, 0x03, 0x00, 0x99,
        ];

        let result = DiagnosticDataBuilder::default().parse(&bytes).unwrap().build();

        assert_eq!(result.total_smoking_count, Some(1234));
        assert_eq!(result.days_used, Some(42));
        assert_eq!(
            result.telemetry_tags,
            vec![
                TelemetryTag { tag: 0x8E, value: 1234 },
                TelemetryTag { tag: 0x17, value: 42 },
                TelemetryTag { tag: 0x33, value: 7 },
                TelemetryTag { tag: 0x99, value: 999 },
            ]
        );
    }

    #[test]
    fn telemetry_tags_accumulate_across_repeated_telemetry_frames() {
        #[rustfmt::skip]
        let bytes: [u8; 38] = [
            0x00, 0x00, 0x90, 0x22, 0x01, 0x01,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x8E,
            0x00, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x17,
            0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x33,
            0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x99,
        ];

        let result = DiagnosticDataBuilder::default()
            .parse(&bytes)
            .unwrap()
            .parse(&bytes)
            .unwrap()
            .build();

        // ALL_DIAGNOSIS_COMMANDS sends LOAD_TELEMETRY_COMMAND twice, so a
        // real diagnosis read accumulates two frames' worth of tags.
        assert_eq!(result.telemetry_tags.len(), 8);
    }
}
