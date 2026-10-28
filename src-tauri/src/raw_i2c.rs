//! Vendor-neutral raw DDC/CI transport.
//!
//! Standard DDC/CI APIs hard-code the host source address `0x51`. Some firmware
//! (notably LG) only reacts to alternate commands sent from source address
//! `0x50`, for example VCP `0xF4`. The raw backend writes the DDC/CI packet
//! directly over the GPU I2C bus so the source address can be chosen per write.

use crate::transport::{TransportError, VcpReading};

pub const DDC_SOURCE_DEFAULT: u8 = 0x51;
pub const DDC_SOURCE_ALTERNATE: u8 = 0x50;

/// Availability of a raw I2C backend for one display.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RawSupport {
    pub available: bool,
    pub backend: Option<String>,
    pub reason: Option<String>,
}

impl RawSupport {
    pub fn unavailable(backend: Option<&str>, reason: impl Into<String>) -> Self {
        Self {
            available: false,
            backend: backend.map(Into::into),
            reason: Some(reason.into()),
        }
    }

    pub fn available(backend: impl Into<String>) -> Self {
        Self {
            available: true,
            backend: Some(backend.into()),
            reason: None,
        }
    }
}

/// A GPU-specific raw I2C handle for one display output.
pub trait RawI2cDevice {
    fn backend(&self) -> &'static str;
    /// Human-readable output description used in diagnostics and errors.
    fn detail(&self) -> String;
    fn read(&self, source: u8, code: u8) -> Result<VcpReading, TransportError>;
    fn write(&self, source: u8, code: u8, value: u16) -> Result<(), TransportError>;
}

pub struct RawAttachment {
    pub device: Option<Box<dyn RawI2cDevice>>,
    pub support: RawSupport,
}

/// Build the DDC/CI "Get VCP Feature" request payload.
pub fn read_request(source: u8, code: u8) -> [u8; 4] {
    let checksum = checksum(source, &[0x82, 0x01, code]);
    [0x82, 0x01, code, checksum]
}

/// Build the DDC/CI "Set VCP Feature" payload.
pub fn write_packet(source: u8, code: u8, value: u16) -> [u8; 6] {
    let bytes = [0x84, 0x03, code, (value >> 8) as u8, value as u8];
    let checksum = checksum(source, &bytes);
    [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], checksum]
}

fn checksum(source: u8, data: &[u8]) -> u8 {
    data.iter().fold(0x6E ^ source, |sum, byte| sum ^ byte)
}

/// Parse a 12-byte DDC/CI reply read from the monitor.
pub fn parse_reply(_source: u8, code: u8, data: &[u8]) -> Result<VcpReading, TransportError> {
    if data.len() < 3 {
        return Err(TransportError::communication("DDC/CI 回复长度不足"));
    }
    if data[1] & 0x80 == 0 {
        return Err(TransportError::communication(format!(
            "DDC/CI 回复缺少长度标志：0x{:02X}",
            data[1]
        )));
    }

    let length = (data[1] & 0x7F) as usize;
    if length < 8 || data.len() < 2 + length + 1 {
        return Err(TransportError::communication(format!(
            "DDC/CI 回复长度无效：{length}"
        )));
    }

    let payload = &data[2..2 + length];
    // DDC/CI reply checksum covers the monitor address 0x6E, the host address
    // 0x50, the length byte and the payload. The host address is constant in
    // replies even when the request used an alternate source address.
    let expected = 0x6E ^ 0x50 ^ data[1] ^ payload.iter().fold(0u8, |sum, byte| sum ^ byte);
    if data[2 + length] != expected {
        return Err(TransportError::communication(format!(
            "DDC/CI 回复校验和错误：期望 0x{expected:02X}，实际 0x{:02X}",
            data[2 + length]
        )));
    }

    if payload[0] != 0x02 {
        return Err(TransportError::communication(format!(
            "DDC/CI 回复操作码无效：0x{:02X}",
            payload[0]
        )));
    }
    if payload[2] != code {
        return Err(TransportError::communication(format!(
            "DDC/CI 回复 VCP 代码不匹配：期望 0x{code:02X}，实际 0x{:02X}",
            payload[2]
        )));
    }
    match payload[1] {
        0x00 => {}
        0x01 => {
            return Err(TransportError::unsupported(format!(
                "显示器不支持 VCP 0x{code:02X}（原始 I2C）"
            )))
        }
        result => {
            return Err(TransportError::communication(format!(
                "显示器拒绝原始 I2C 请求：结果码 0x{result:02X}"
            )))
        }
    }

    let maximum = u16::from_be_bytes([payload[4], payload[5]]);
    let value = u16::from_be_bytes([payload[6], payload[7]]);
    Ok(VcpReading {
        value,
        maximum,
        ty: payload[3],
        sh: payload[6],
        sl: payload[7],
    })
}

/// Attach raw I2C devices to the enumerated displays.
pub fn attach(display_count: usize) -> Vec<RawAttachment> {
    #[cfg(windows)]
    {
        windows::attach(display_count)
    }
    #[cfg(not(windows))]
    {
        unsupported_attachments(display_count, platform_reason())
    }
}

#[cfg(not(windows))]
fn platform_reason() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "macOS 的原始 I2C 后端尚未实现（需要 IOAVService），标准 DDC/CI 仍可使用。"
    }
    #[cfg(target_os = "linux")]
    {
        "Linux 的原始 I2C 后端尚未实现，标准 DDC/CI 仍可使用。"
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        "当前平台没有可用的原始 I2C 后端。"
    }
}

#[cfg(not(windows))]
fn unsupported_attachments(display_count: usize, reason: &str) -> Vec<RawAttachment> {
    (0..display_count)
        .map(|_| RawAttachment {
            device: None,
            support: RawSupport::unavailable(None, reason),
        })
        .collect()
}

#[cfg(windows)]
mod windows {
    #![allow(improper_ctypes_definitions)]

    use std::ffi::c_char;
    use std::mem::{size_of, transmute_copy};
    use std::sync::OnceLock;
    use std::time::Duration;

    use nvapi_sys::handles::{NvDisplayHandle, NvPhysicalGpuHandle};
    use nvapi_sys::i2c::{
        NvAPI_I2CRead, NvAPI_I2CWrite, NVAPI_I2C_SPEED_DEPRECATED, NV_I2C_INFO, NV_I2C_INFO_VER,
    };
    use nvapi_sys::nvapi::NvAPI_Initialize;
    use nvapi_sys::status::{Status, NVAPI_END_ENUMERATION, NVAPI_OK};
    use windows_sys::Win32::Devices::Display::GetNumberOfPhysicalMonitorsFromHMONITOR;
    use windows_sys::Win32::Foundation::{BOOL, LPARAM, RECT, TRUE};
    use windows_sys::Win32::Graphics::Gdi::{
        EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW,
    };

    use super::{parse_reply, read_request, write_packet, RawAttachment, RawI2cDevice, RawSupport};
    use crate::transport::{TransportError, VcpReading};

    const ID_GET_PHYSICAL_GPUS: u32 = 0x34EF9506;
    const ID_CONNECTED_OUTPUTS: u32 = 0x1730BFC9;
    const ID_ASSOCIATED_DISPLAY_NAME: u32 = 0x22A78B05;

    type GetPhysicalGpusFn =
        extern "C" fn(NvDisplayHandle, *mut NvPhysicalGpuHandle, *mut u32) -> i32;
    type GetConnectedOutputsFn = extern "C" fn(NvPhysicalGpuHandle, *mut u32) -> i32;
    type GetDisplayNameFn = extern "C" fn(NvDisplayHandle, *mut c_char) -> i32;

    struct NvapiDisplay {
        name: String,
        gpu: NvPhysicalGpuHandle,
        output_mask: u32,
    }

    struct NvapiI2cDevice {
        gpu: NvPhysicalGpuHandle,
        display_mask: u32,
        detail: String,
    }

    impl RawI2cDevice for NvapiI2cDevice {
        fn backend(&self) -> &'static str {
            "nvapi"
        }

        fn detail(&self) -> String {
            self.detail.clone()
        }

        fn read(&self, source: u8, code: u8) -> Result<VcpReading, TransportError> {
            read_vcp(self.gpu, self.display_mask, source, code)
        }

        fn write(&self, source: u8, code: u8, value: u16) -> Result<(), TransportError> {
            let packet = write_packet(source, code, value);
            write_i2c(self.gpu, self.display_mask, source, &packet)
        }
    }

    pub fn attach(display_count: usize) -> Vec<RawAttachment> {
        if display_count == 0 {
            return Vec::new();
        }

        let nvapi_displays = match enumerate_nvapi() {
            Ok(displays) if !displays.is_empty() => displays,
            Ok(_) => {
                return unavailable_all(display_count, "NVAPI 未发现由 NVIDIA GPU 驱动的显示器。")
            }
            Err(reason) => return unavailable_all(display_count, &reason),
        };

        let gdi_names = gdi_monitor_names().unwrap_or_default();
        let mut assignments = vec![None; display_count];
        let mut used = vec![false; nvapi_displays.len()];

        for (index, device) in gdi_names.iter().take(display_count).enumerate() {
            if let Some((nv_index, _)) = nvapi_displays
                .iter()
                .enumerate()
                .find(|(nv_index, nv)| !used[*nv_index] && names_equal(&nv.name, device))
            {
                assignments[index] = Some(nv_index);
                used[nv_index] = true;
            }
        }

        // NVAPI display handles and Windows display enumeration both follow the
        // display index order. Fall back to positional matching only when the
        // counts line up, which keeps single-display systems deterministic.
        if assignments.iter().all(Option::is_none) && nvapi_displays.len() == display_count {
            for (index, assignment) in assignments.iter_mut().enumerate() {
                *assignment = Some(index);
                used[index] = true;
            }
        }

        let selected_mask: Vec<u32> = nvapi_displays
            .iter()
            .map(|display| choose_output_mask(display.gpu, display.output_mask))
            .collect();

        assignments
            .into_iter()
            .map(|assignment| match assignment {
                Some(index) => {
                    let display = &nvapi_displays[index];
                    let mask = selected_mask[index];
                    RawAttachment {
                        device: Some(Box::new(NvapiI2cDevice {
                            gpu: display.gpu,
                            display_mask: mask,
                            detail: format!(
                                "{} (NVAPI output mask 0x{mask:08X})",
                                if display.name.is_empty() {
                                    "NVIDIA display"
                                } else {
                                    display.name.as_str()
                                }
                            ),
                        })),
                        support: RawSupport::available("nvapi"),
                    }
                }
                None => RawAttachment {
                    device: None,
                    support: RawSupport::unavailable(
                        Some("nvapi"),
                        "未能把该显示器映射到唯一的 NVIDIA 输出；它可能连接在核显或其他厂商 GPU 上。",
                    ),
                },
            })
            .collect()
    }

    fn unavailable_all(display_count: usize, reason: &str) -> Vec<RawAttachment> {
        (0..display_count)
            .map(|_| RawAttachment {
                device: None,
                support: RawSupport::unavailable(Some("nvapi"), reason),
            })
            .collect()
    }

    fn ensure_nvapi() -> Result<(), String> {
        static INIT: OnceLock<Result<(), String>> = OnceLock::new();
        INIT.get_or_init(|| unsafe {
            let status = NvAPI_Initialize();
            if status == NVAPI_OK {
                Ok(())
            } else {
                Err(format!(
                    "NVAPI 初始化失败（{}）；仅 NVIDIA 显卡提供可用的原始 I2C 接口。",
                    status_name(status)
                ))
            }
        })
        .clone()
    }

    fn enumerate_nvapi() -> Result<Vec<NvapiDisplay>, String> {
        ensure_nvapi()?;

        unsafe {
            let get_physical: GetPhysicalGpusFn = query_function(ID_GET_PHYSICAL_GPUS)
                .ok_or_else(|| "NVAPI 缺少 GetPhysicalGPUsFromDisplay".to_string())?;
            let get_outputs: GetConnectedOutputsFn = query_function(ID_CONNECTED_OUTPUTS)
                .ok_or_else(|| "NVAPI 缺少 GPU_GetConnectedOutputs".to_string())?;
            let get_name: GetDisplayNameFn = query_function(ID_ASSOCIATED_DISPLAY_NAME)
                .ok_or_else(|| "NVAPI 缺少 GetAssociatedNvidiaDisplayName".to_string())?;

            let mut displays = Vec::new();
            for index in 0..64 {
                let mut display = NvDisplayHandle::default();
                let status =
                    nvapi_sys::dispcontrol::NvAPI_EnumNvidiaDisplayHandle(index, &mut display);
                if status == NVAPI_END_ENUMERATION {
                    break;
                }
                if status != NVAPI_OK {
                    return Err(format!("NVAPI 枚举显示器失败：{}", status_name(status)));
                }

                let mut name_buffer = [0 as c_char; 64];
                let name = if get_name(display, name_buffer.as_mut_ptr()) == NVAPI_OK {
                    string_from_c_buffer(&name_buffer)
                } else {
                    String::new()
                };

                let mut gpus = [NvPhysicalGpuHandle::default(); 64];
                let mut count = 0u32;
                if get_physical(display, gpus.as_mut_ptr(), &mut count) != NVAPI_OK || count == 0 {
                    continue;
                }

                let mut mask = 0u32;
                if get_outputs(gpus[0], &mut mask) != NVAPI_OK || mask == 0 {
                    continue;
                }

                displays.push(NvapiDisplay {
                    name,
                    gpu: gpus[0],
                    output_mask: mask,
                });
            }

            Ok(displays)
        }
    }

    unsafe fn query_function<T>(id: u32) -> Option<T> {
        let ptr = nvapi_sys::nvapi_QueryInterface(id).ok()?;
        debug_assert_eq!(size_of::<T>(), size_of::<usize>());
        Some(transmute_copy(&ptr))
    }

    fn choose_output_mask(gpu: NvPhysicalGpuHandle, mask: u32) -> u32 {
        let bits: Vec<u32> = (0..32)
            .filter(|bit| mask & (1 << bit) != 0)
            .map(|bit| 1u32 << bit)
            .collect();
        match bits.as_slice() {
            [] => mask,
            [only] => *only,
            _ => bits
                .iter()
                .copied()
                .find(|bit| read_vcp(gpu, *bit, super::DDC_SOURCE_DEFAULT, 0x60).is_ok())
                .unwrap_or(bits[0]),
        }
    }

    fn names_equal(left: &str, right: &str) -> bool {
        !left.is_empty() && normalize_display_name(left) == normalize_display_name(right)
    }

    fn normalize_display_name(name: &str) -> String {
        name.trim()
            .trim_start_matches("\\\\.\\")
            .trim_start_matches("\\\\")
            .trim_end_matches('\0')
            .to_ascii_uppercase()
    }

    fn string_from_c_buffer(buffer: &[c_char]) -> String {
        let bytes: Vec<u8> = buffer
            .iter()
            .take_while(|byte| **byte != 0)
            .map(|byte| *byte as u8)
            .collect();
        String::from_utf8_lossy(&bytes).trim().to_string()
    }

    fn gdi_monitor_names() -> Result<Vec<String>, String> {
        unsafe extern "system" fn callback(
            monitor: HMONITOR,
            _hdc: HDC,
            _rect: *mut RECT,
            data: LPARAM,
        ) -> BOOL {
            let monitors = &mut *(data as *mut Vec<HMONITOR>);
            monitors.push(monitor);
            TRUE
        }

        let mut monitors: Vec<HMONITOR> = Vec::new();
        let ok = unsafe {
            EnumDisplayMonitors(
                std::ptr::null_mut(),
                std::ptr::null(),
                Some(callback),
                &mut monitors as *mut _ as LPARAM,
            )
        };
        if ok == 0 {
            return Err("枚举 Windows 显示器失败。".into());
        }

        let mut names = Vec::new();
        for monitor in monitors {
            let mut info: MONITORINFOEXW = unsafe { std::mem::zeroed() };
            info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
            let ok = unsafe { GetMonitorInfoW(monitor, &mut info as *mut _ as *mut MONITORINFO) };
            if ok == 0 {
                continue;
            }
            let wide: Vec<u16> = info
                .szDevice
                .iter()
                .take_while(|ch| **ch != 0)
                .copied()
                .collect();
            let device = String::from_utf16_lossy(&wide);
            let mut physical_count = 0u32;
            unsafe {
                GetNumberOfPhysicalMonitorsFromHMONITOR(monitor, &mut physical_count);
            }
            for _ in 0..physical_count.max(1) {
                names.push(device.clone());
            }
        }
        Ok(names)
    }

    fn read_vcp(
        gpu: NvPhysicalGpuHandle,
        display_mask: u32,
        source: u8,
        code: u8,
    ) -> Result<VcpReading, TransportError> {
        let request = read_request(source, code);
        write_i2c(gpu, display_mask, source, &request)?;
        std::thread::sleep(Duration::from_millis(40));

        let mut data = [0u8; 12];
        read_i2c(gpu, display_mask, source, &mut data)?;
        parse_reply(source, code, &data)
    }

    fn write_i2c(
        gpu: NvPhysicalGpuHandle,
        display_mask: u32,
        source: u8,
        data: &[u8],
    ) -> Result<(), TransportError> {
        let mut register = [source];
        let mut info = i2c_info(
            display_mask,
            &mut register,
            data.as_ptr() as *mut u8,
            data.len() as u32,
        );
        let status = unsafe { NvAPI_I2CWrite(gpu, &mut info) };
        check_status("NVAPI I2C 写入", status)
    }

    fn read_i2c(
        gpu: NvPhysicalGpuHandle,
        display_mask: u32,
        source: u8,
        data: &mut [u8],
    ) -> Result<(), TransportError> {
        let mut register = [source];
        let mut info = i2c_info(
            display_mask,
            &mut register,
            data.as_mut_ptr(),
            data.len() as u32,
        );
        let status = unsafe { NvAPI_I2CRead(gpu, &mut info) };
        check_status("NVAPI I2C 读取", status)
    }

    fn i2c_info(display_mask: u32, register: &mut [u8; 1], data: *mut u8, len: u32) -> NV_I2C_INFO {
        NV_I2C_INFO {
            version: NV_I2C_INFO_VER,
            displayMask: display_mask,
            bIsDDCPort: 1,
            i2cDevAddress: 0x6E,
            pbI2cRegAddress: register.as_mut_ptr(),
            regAddrSize: 1,
            pbData: data,
            cbSize: len,
            i2cSpeed: NVAPI_I2C_SPEED_DEPRECATED,
            i2cSpeedKhz: Default::default(),
            portId: 0,
            bIsPortIdSet: 0,
        }
    }

    fn check_status(action: &str, status: i32) -> Result<(), TransportError> {
        if status == NVAPI_OK {
            Ok(())
        } else {
            Err(TransportError::communication(format!(
                "{action}失败：{}",
                status_name(status)
            )))
        }
    }

    fn status_name(status: i32) -> String {
        Status::from_raw(status)
            .map(|status| format!("{status:?} ({status})"))
            .unwrap_or_else(|_| format!("状态 {status}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_packet_uses_source_address_in_checksum() {
        let default = write_packet(DDC_SOURCE_DEFAULT, 0x60, 0x0F);
        let alternate = write_packet(DDC_SOURCE_ALTERNATE, 0x60, 0x0F);
        assert_eq!(&default[..5], &[0x84, 0x03, 0x60, 0x00, 0x0F]);
        assert_ne!(default[5], alternate[5]);
    }

    #[test]
    fn parses_standard_ddc_reply() {
        let reply = [
            0x6E, 0x88, 0x02, 0x00, 0x60, 0x00, 0x00, 0x12, 0x00, 0x0F, 0xC9, 0x00,
        ];
        let reading = parse_reply(DDC_SOURCE_DEFAULT, 0x60, &reply).unwrap();
        assert_eq!(reading.value, 0x0F);
        assert_eq!(reading.maximum, 0x12);
    }

    #[test]
    fn parses_alternate_address_reply() {
        let reply = [
            0x6E, 0x88, 0x02, 0x00, 0xF4, 0x00, 0xFF, 0xFF, 0x00, 0x07, 0x47, 0x00,
        ];
        let reading = parse_reply(DDC_SOURCE_ALTERNATE, 0xF4, &reply).unwrap();
        assert_eq!(reading.value, 7);
        assert_eq!(reading.maximum, 0xFFFF);
    }

    #[test]
    fn rejects_corrupt_reply() {
        let mut reply = [
            0x6E, 0x88, 0x02, 0x00, 0x60, 0x00, 0x00, 0x12, 0x00, 0x0F, 0xC9, 0x00,
        ];
        reply[10] ^= 0xFF;
        assert!(parse_reply(DDC_SOURCE_DEFAULT, 0x60, &reply).is_err());
    }

    #[test]
    fn reports_unsupported_vcp_result() {
        let mut reply = [
            0x6E, 0x88, 0x02, 0x00, 0x60, 0x00, 0x00, 0x12, 0x00, 0x0F, 0xC9, 0x00,
        ];
        reply[3] = 0x01;
        reply[10] ^= 0x01;
        let error = parse_reply(DDC_SOURCE_DEFAULT, 0x60, &reply).unwrap_err();
        assert!(error.message.contains("不支持"));
    }
}
