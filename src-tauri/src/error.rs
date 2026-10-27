use serde::Serialize;

use crate::transport::{TransportError, TransportErrorKind};

/// Error shape consumed by the UI. Every error is actionable and non-fatal.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub hint: Option<String>,
}

impl AppError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new("internal", message)
    }

    pub fn timeout() -> Self {
        Self::new("timeout", "显示器在规定时间内没有响应。")
            .with_hint("检查连接线/扩展坞，确认显示器未休眠；随后点击“刷新状态”重试。")
    }

    pub fn no_displays(detail: Option<String>) -> Self {
        let mut error = Self::new(
            "no_displays",
            "未发现可通过 DDC/CI 控制的显示器。",
        )
        .with_hint(
            "确认显示器已唤醒并在 OSD 中开启 DDC/CI；优先使用直连 HDMI/DP/Type-C，避免不支持 DDC 的扩展坞或 KVM。",
        );
        if let Some(detail) = detail {
            error.hint = Some(format!(
                "{} 底层信息：{}",
                error.hint.unwrap_or_default(),
                detail
            ));
        }
        error
    }

    pub fn display_not_found(id: &str) -> Self {
        Self::new("display_not_found", format!("显示器 {id} 已不在线。"))
            .with_hint("切换连接后请点击“刷新”；应用会在刷新时重新枚举显示器。")
    }

    pub fn switch_failed(error: &TransportError) -> Self {
        let mut app = Self::from_transport("切换输入源", error);
        app.hint = Some(
            "常见原因：当前主机不是显示器的活动输入源、显示器未开启 DDC/CI、接口/扩展坞不支持 DDC/CI，或目标输入没有信号。".into(),
        );
        app
    }

    pub fn from_transport(action: &str, error: &TransportError) -> Self {
        let (message, hint) = match error.kind {
            TransportErrorKind::Permission => (
                format!("没有权限访问显示器（{action}）。"),
                Some("检查系统隐私/安全设置，或换用直连端口后重试。".to_string()),
            ),
            TransportErrorKind::Unsupported => (
                format!("显示器不支持该操作（{action}）。"),
                Some("可在显示器配置中调整 VCP 码与支持状态，或运行诊断确认实际能力。".to_string()),
            ),
            TransportErrorKind::Timeout => (
                format!("显示器响应超时（{action}）。"),
                Some("确认显示器未休眠、DDC/CI 已开启，并避免通过低质量扩展坞连接。".to_string()),
            ),
            TransportErrorKind::NotConnected => (
                format!("显示器连接已中断（{action}）。"),
                Some(
                    "输入源切换后当前主机可能已失去显示信号，请在新的活动主机上打开 DisplayPilot。"
                        .to_string(),
                ),
            ),
            TransportErrorKind::Communication => (
                format!("DDC/CI 通信失败（{action}）。"),
                Some("检查线缆与转接器；部分 KVM/扩展坞不会透传 DDC/CI。".to_string()),
            ),
            TransportErrorKind::Other => (
                format!("无法完成操作（{action}）。"),
                Some("查看诊断信息和日志以获取底层错误。".to_string()),
            ),
        };

        Self {
            code: match error.kind {
                TransportErrorKind::Permission => "permission",
                TransportErrorKind::Unsupported => "unsupported",
                TransportErrorKind::Timeout => "timeout",
                TransportErrorKind::NotConnected => "display_not_found",
                TransportErrorKind::Communication => "ddc_error",
                TransportErrorKind::Other => "internal",
            }
            .into(),
            message,
            hint,
        }
    }

    pub fn config(message: impl Into<String>) -> Self {
        Self::new("invalid_config", message)
            .with_hint("修正配置文件后点击“重新加载配置”，内置配置会在用户配置无效时继续生效。")
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

impl From<TransportError> for AppError {
    fn from(value: TransportError) -> Self {
        Self::from_transport("DDC/CI 操作", &value)
    }
}
