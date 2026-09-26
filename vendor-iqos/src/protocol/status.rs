use crate::protocol::{DeviceInfo, DeviceModel, FirmwareVersion};

/// Aggregated device snapshot combining GATT metadata and SCP firmware reads.
///
/// Populated by [`Iqos::read_device_status`](crate::Iqos::read_device_status).
/// For models where
/// [`DeviceModel::supports_holder_features`](crate::protocol::DeviceModel::supports_holder_features)
/// returns `true`, holder product number and firmware fields are `Some`. For
/// one-piece models they are `None`.
/// `battery_voltage` is `None` when the SCP transport read fails.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceStatus {
    /// Detected device model.
    pub model: DeviceModel,
    /// Standard BLE device-information fields read at connection time.
    pub device_info: DeviceInfo,
    /// Product number reported by the stick, or by the device itself on one-piece models.
    pub product_number: String,
    /// Firmware version reported by the stick, or by the device itself on one-piece models.
    pub stick_firmware: FirmwareVersion,
    /// Full raw response frame the stick firmware version was decoded from.
    ///
    /// [`FirmwareVersion::from_response`] only decodes 4 of this frame's
    /// bytes; the rest (a handful of leading bytes plus a longer trailing
    /// block observed on real hardware) are not yet understood. Exposed for
    /// inspection rather than discarded.
    pub stick_firmware_raw: Vec<u8>,
    /// Product number reported by the holder when
    /// [`DeviceModel::supports_holder_features`](crate::protocol::DeviceModel::supports_holder_features)
    /// returns `true`.
    pub holder_product_number: Option<String>,
    /// Firmware version reported by the holder when
    /// [`DeviceModel::supports_holder_features`](crate::protocol::DeviceModel::supports_holder_features)
    /// returns `true`.
    pub holder_firmware: Option<FirmwareVersion>,
    /// Full raw response frame the holder firmware version was decoded from,
    /// when applicable. See [`DeviceStatus::stick_firmware_raw`].
    pub holder_firmware_raw: Option<Vec<u8>>,
    /// Battery cell voltage in volts (e.g. `4.2`), or `None` if the SCP transport read failed.
    pub battery_voltage: Option<f32>,
    /// Full raw response frame the battery voltage was decoded from, when
    /// the read succeeded. Only the first few bytes are decoded into
    /// [`DeviceStatus::battery_voltage`]; on real hardware the rest of this
    /// frame carries additional undecoded, non-zero data.
    pub battery_voltage_raw: Option<Vec<u8>>,
}
