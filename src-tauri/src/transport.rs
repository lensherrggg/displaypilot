use std::fmt;

use ddc_hi::{Ddc, DdcTable, Display as HiDisplay};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportErrorKind {
    Unsupported,
    Permission,
    Timeout,
    NotConnected,
    Communication,
    Other,
}

#[derive(Debug, Clone)]
pub struct TransportError {
    pub kind: TransportErrorKind,
    pub message: String,
}

impl TransportError {
    pub fn new(kind: TransportErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn communication(message: impl Into<String>) -> Self {
        Self::new(TransportErrorKind::Communication, message)
    }

    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(TransportErrorKind::Unsupported, message)
    }

    pub fn from_message(message: String) -> Self {
        let lower = message.to_lowercase();
        let kind = if lower.contains("not supported")
            || lower.contains("unsupported")
            || lower.contains("null message")
        {
            TransportErrorKind::Unsupported
        } else if lower.contains("permission")
            || lower.contains("denied")
            || lower.contains("access is")
            || lower.contains("not authorized")
        {
            TransportErrorKind::Permission
        } else if lower.contains("timeout") || lower.contains("timed out") {
            TransportErrorKind::Timeout
        } else if lower.contains("not found")
            || lower.contains("disconnected")
            || lower.contains("no longer")
            || lower.contains("invalid handle")
        {
            TransportErrorKind::NotConnected
        } else if lower.contains("ddc")
            || lower.contains("i2c")
            || lower.contains("checksum")
            || lower.contains("invalid")
        {
            TransportErrorKind::Communication
        } else {
            TransportErrorKind::Other
        };
        Self::new(kind, message)
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for TransportError {}

impl From<anyhow::Error> for TransportError {
    fn from(value: anyhow::Error) -> Self {
        Self::from_message(format!("{value:#}"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VcpReading {
    pub value: u16,
    pub maximum: u16,
    pub ty: u8,
    pub sh: u8,
    pub sl: u8,
}

pub struct RawDisplay<H> {
    pub handle: H,
    pub backend: String,
    pub native_id: String,
    pub vendor_id: Option<String>,
    pub model_id: Option<u16>,
    pub model_name: Option<String>,
    pub serial_number: Option<String>,
    pub edid_serial: Option<u32>,
    pub edid: Option<Vec<u8>>,
    pub capabilities_error: Option<String>,
}

pub trait DdcTransport {
    type Handle;

    fn enumerate(&self) -> Result<Vec<RawDisplay<Self::Handle>>, TransportError>;

    fn read_vcp(&self, display: &mut Self::Handle, code: u8) -> Result<VcpReading, TransportError>;

    fn write_vcp(
        &self,
        display: &mut Self::Handle,
        code: u8,
        value: u16,
    ) -> Result<(), TransportError>;

    fn table_write(
        &self,
        display: &mut Self::Handle,
        code: u8,
        offset: u16,
        data: &[u8],
    ) -> Result<(), TransportError>;

    fn capabilities_string(&self, display: &mut Self::Handle) -> Result<String, TransportError>;
}

#[derive(Default)]
pub struct HardwareTransport;

pub struct HardwareDisplay {
    inner: HiDisplay,
}

impl DdcTransport for HardwareTransport {
    type Handle = HardwareDisplay;

    fn enumerate(&self) -> Result<Vec<RawDisplay<Self::Handle>>, TransportError> {
        let displays = HiDisplay::enumerate();
        log::info!("DDC enumeration found {} display(s)", displays.len());

        let mut result = Vec::with_capacity(displays.len());
        for mut display in displays {
            let capabilities_error = match display.update_capabilities() {
                Ok(()) => None,
                Err(error) => {
                    log::warn!("Capabilities probe failed for {}: {error:#}", display.info);
                    Some(format!("{error:#}"))
                }
            };

            let info = &display.info;
            log::info!(
                "display backend={} id={} vendor={:?} model_id={:?} model={:?} serial={:?}",
                info.backend,
                info.id,
                info.manufacturer_id,
                info.model_id,
                info.model_name,
                info.serial_number
            );

            result.push(RawDisplay {
                backend: info.backend.to_string(),
                native_id: info.id.clone(),
                vendor_id: info.manufacturer_id.clone(),
                model_id: info.model_id,
                model_name: info.model_name.clone(),
                serial_number: info.serial_number.clone(),
                edid_serial: info.serial,
                edid: info.edid_data.clone(),
                capabilities_error,
                handle: HardwareDisplay { inner: display },
            });
        }

        Ok(result)
    }

    fn read_vcp(&self, display: &mut Self::Handle, code: u8) -> Result<VcpReading, TransportError> {
        let value = display
            .inner
            .handle
            .get_vcp_feature(code)
            .map_err(TransportError::from)?;
        Ok(VcpReading {
            value: value.value(),
            maximum: value.maximum(),
            ty: value.ty,
            sh: value.sh,
            sl: value.sl,
        })
    }

    fn write_vcp(
        &self,
        display: &mut Self::Handle,
        code: u8,
        value: u16,
    ) -> Result<(), TransportError> {
        display
            .inner
            .handle
            .set_vcp_feature(code, value)
            .map_err(TransportError::from)
    }

    fn table_write(
        &self,
        display: &mut Self::Handle,
        code: u8,
        offset: u16,
        data: &[u8],
    ) -> Result<(), TransportError> {
        display
            .inner
            .handle
            .table_write(code, offset, data)
            .map_err(TransportError::from)
    }

    fn capabilities_string(&self, display: &mut Self::Handle) -> Result<String, TransportError> {
        let bytes = display
            .inner
            .handle
            .capabilities_string()
            .map_err(TransportError::from)?;
        Ok(String::from_utf8_lossy(&bytes)
            .trim_end_matches('\0')
            .trim()
            .to_string())
    }
}
