use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use crate::config::AppPaths;
use crate::error::AppError;
use crate::models::{
    AppInfo, BootstrapPayload, ConfigInfo, DiagnosticReport, DisplayList, DisplaySnapshot,
};
use crate::service::{DisplayService, QuickAction};
use crate::transport::HardwareTransport;

/// DDC/CI calls can block for a long time on flaky links. The UI must stay responsive,
/// so every request is bounded and a timed-out job is left to finish on the worker.
/// Discovery and diagnostics chain several slow reads (capabilities, 0x60/0x62/0x8D),
/// so they need a much larger budget than a single feature access.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(20);
const DIAGNOSTICS_TIMEOUT: Duration = Duration::from_secs(30);

pub enum Command {
    AppInfo,
    Bootstrap,
    ListDisplays {
        force: bool,
    },
    SelectDisplay {
        display_id: String,
    },
    Snapshot {
        display_id: String,
        force: bool,
    },
    SwitchInput {
        display_id: String,
        input_id: String,
    },
    SetBrightness {
        display_id: String,
        percent: u8,
    },
    SetVolume {
        display_id: String,
        percent: u8,
    },
    ToggleMute {
        display_id: String,
    },
    SetMute {
        display_id: String,
        muted: bool,
    },
    QuickAction {
        action: QuickAction,
    },
    ConfigInfo,
    ReloadConfig,
    Diagnostics {
        display_id: String,
    },
}

pub enum Response {
    Ack,
    AppInfo(AppInfo),
    Bootstrap(BootstrapPayload),
    Displays(DisplayList),
    Snapshot(DisplaySnapshot),
    Config(ConfigInfo),
    Diagnostics(Box<DiagnosticReport>),
}

impl Command {
    fn timeout(&self) -> Duration {
        match self {
            Command::Bootstrap | Command::ListDisplays { .. } | Command::ReloadConfig => {
                DISCOVERY_TIMEOUT
            }
            Command::Diagnostics { .. } => DIAGNOSTICS_TIMEOUT,
            Command::AppInfo
            | Command::SelectDisplay { .. }
            | Command::Snapshot { .. }
            | Command::SwitchInput { .. }
            | Command::SetBrightness { .. }
            | Command::SetVolume { .. }
            | Command::ToggleMute { .. }
            | Command::SetMute { .. }
            | Command::QuickAction { .. }
            | Command::ConfigInfo => DEFAULT_TIMEOUT,
        }
    }

    fn execute(
        self,
        service: &mut DisplayService<HardwareTransport>,
    ) -> Result<Response, AppError> {
        match self {
            Command::AppInfo => Ok(Response::AppInfo(service.app_info())),
            Command::Bootstrap => Ok(Response::Bootstrap(BootstrapPayload {
                app: service.app_info(),
                config: service.config_info(),
                displays: service.list_displays(false),
            })),
            Command::ListDisplays { force } => Ok(Response::Displays(service.list_displays(force))),
            Command::SelectDisplay { display_id } => {
                service.select_display(&display_id)?;
                Ok(Response::Ack)
            }
            Command::Snapshot { display_id, force } => {
                Ok(Response::Snapshot(service.snapshot(&display_id, force)?))
            }
            Command::SwitchInput {
                display_id,
                input_id,
            } => Ok(Response::Snapshot(
                service.switch_input(&display_id, &input_id)?,
            )),
            Command::SetBrightness {
                display_id,
                percent,
            } => Ok(Response::Snapshot(
                service.set_brightness(&display_id, percent)?,
            )),
            Command::SetVolume {
                display_id,
                percent,
            } => Ok(Response::Snapshot(
                service.set_volume(&display_id, percent)?,
            )),
            Command::ToggleMute { display_id } => {
                Ok(Response::Snapshot(service.toggle_mute(&display_id)?))
            }
            Command::SetMute { display_id, muted } => {
                Ok(Response::Snapshot(service.set_mute(&display_id, muted)?))
            }
            Command::QuickAction { action } => {
                Ok(Response::Snapshot(service.quick_action(&action)?))
            }
            Command::ConfigInfo => Ok(Response::Config(service.config_info())),
            Command::ReloadConfig => Ok(Response::Config(service.reload_config()?)),
            Command::Diagnostics { display_id } => Ok(Response::Diagnostics(Box::new(
                service.diagnostics(&display_id)?,
            ))),
        }
    }
}

struct Job {
    command: Command,
    reply: Sender<Result<Response, AppError>>,
}

/// Cloneable handle used by Tauri commands and shortcut handlers. All hardware access is
/// serialized on one dedicated thread because DDC handles are not `Send` on every backend.
#[derive(Clone)]
pub struct WorkerClient {
    sender: Sender<Job>,
}

impl WorkerClient {
    pub fn spawn(paths: AppPaths) -> Self {
        let (sender, receiver) = mpsc::channel::<Job>();
        match thread::Builder::new()
            .name("displaypilot-ddc".into())
            .spawn(move || worker_loop(paths, receiver))
        {
            Ok(_) => {}
            Err(error) => log::error!("Failed to spawn DDC worker thread: {error}"),
        }
        Self { sender }
    }

    pub fn call(&self, command: Command) -> Result<Response, AppError> {
        let timeout = command.timeout();
        let (reply, response) = mpsc::channel();
        self.sender
            .send(Job { command, reply })
            .map_err(|_| AppError::internal("显示器工作线程已退出，请重启 DisplayPilot。"))?;

        match response.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => {
                log::warn!("DDC request exceeded {timeout:?}");
                Err(AppError::timeout())
            }
            Err(RecvTimeoutError::Disconnected) => {
                Err(AppError::internal("显示器工作线程未返回结果，请查看日志。"))
            }
        }
    }

    pub fn app_info(&self) -> Result<AppInfo, AppError> {
        expect(self.call(Command::AppInfo)?, |response| match response {
            Response::AppInfo(value) => Some(value),
            _ => None,
        })
    }

    pub fn bootstrap(&self) -> Result<BootstrapPayload, AppError> {
        expect(self.call(Command::Bootstrap)?, |response| match response {
            Response::Bootstrap(value) => Some(value),
            _ => None,
        })
    }

    pub fn list_displays(&self, force: bool) -> Result<DisplayList, AppError> {
        expect(
            self.call(Command::ListDisplays { force })?,
            |response| match response {
                Response::Displays(value) => Some(value),
                _ => None,
            },
        )
    }

    pub fn select_display(&self, display_id: &str) -> Result<(), AppError> {
        self.call(Command::SelectDisplay {
            display_id: display_id.to_string(),
        })
        .map(|_| ())
    }

    pub fn snapshot(&self, display_id: &str, force: bool) -> Result<DisplaySnapshot, AppError> {
        expect(
            self.call(Command::Snapshot {
                display_id: display_id.to_string(),
                force,
            })?,
            |response| match response {
                Response::Snapshot(value) => Some(value),
                _ => None,
            },
        )
    }

    pub fn switch_input(
        &self,
        display_id: &str,
        input_id: &str,
    ) -> Result<DisplaySnapshot, AppError> {
        expect(
            self.call(Command::SwitchInput {
                display_id: display_id.to_string(),
                input_id: input_id.to_string(),
            })?,
            |response| match response {
                Response::Snapshot(value) => Some(value),
                _ => None,
            },
        )
    }

    pub fn set_brightness(
        &self,
        display_id: &str,
        percent: u8,
    ) -> Result<DisplaySnapshot, AppError> {
        expect(
            self.call(Command::SetBrightness {
                display_id: display_id.to_string(),
                percent,
            })?,
            |response| match response {
                Response::Snapshot(value) => Some(value),
                _ => None,
            },
        )
    }

    pub fn set_volume(&self, display_id: &str, percent: u8) -> Result<DisplaySnapshot, AppError> {
        expect(
            self.call(Command::SetVolume {
                display_id: display_id.to_string(),
                percent,
            })?,
            |response| match response {
                Response::Snapshot(value) => Some(value),
                _ => None,
            },
        )
    }

    pub fn toggle_mute(&self, display_id: &str) -> Result<DisplaySnapshot, AppError> {
        expect(
            self.call(Command::ToggleMute {
                display_id: display_id.to_string(),
            })?,
            |response| match response {
                Response::Snapshot(value) => Some(value),
                _ => None,
            },
        )
    }

    pub fn set_mute(&self, display_id: &str, muted: bool) -> Result<DisplaySnapshot, AppError> {
        expect(
            self.call(Command::SetMute {
                display_id: display_id.to_string(),
                muted,
            })?,
            |response| match response {
                Response::Snapshot(value) => Some(value),
                _ => None,
            },
        )
    }

    pub fn quick_action(&self, action: QuickAction) -> Result<DisplaySnapshot, AppError> {
        expect(
            self.call(Command::QuickAction { action })?,
            |response| match response {
                Response::Snapshot(value) => Some(value),
                _ => None,
            },
        )
    }

    pub fn config_info(&self) -> Result<ConfigInfo, AppError> {
        expect(self.call(Command::ConfigInfo)?, |response| match response {
            Response::Config(value) => Some(value),
            _ => None,
        })
    }

    pub fn reload_config(&self) -> Result<ConfigInfo, AppError> {
        expect(
            self.call(Command::ReloadConfig)?,
            |response| match response {
                Response::Config(value) => Some(value),
                _ => None,
            },
        )
    }

    pub fn diagnostics(&self, display_id: &str) -> Result<DiagnosticReport, AppError> {
        expect(
            self.call(Command::Diagnostics {
                display_id: display_id.to_string(),
            })?,
            |response| match response {
                Response::Diagnostics(value) => Some(*value),
                _ => None,
            },
        )
    }
}

fn expect<T>(
    response: Response,
    extract: impl FnOnce(Response) -> Option<T>,
) -> Result<T, AppError> {
    extract(response).ok_or_else(|| AppError::internal("显示器工作线程返回了意外的响应类型。"))
}

fn worker_loop(paths: AppPaths, receiver: Receiver<Job>) {
    let mut service = match DisplayService::new(HardwareTransport, &paths) {
        Ok(service) => Some(service),
        Err(error) => {
            log::error!("Display service initialization failed: {error}");
            None
        }
    };
    let init_error = service
        .is_none()
        .then(|| AppError::internal("显示器服务初始化失败，请查看日志。"));

    for job in receiver {
        let result = match service.as_mut() {
            Some(service) => job.command.execute(service),
            None => Err(init_error
                .clone()
                .unwrap_or_else(|| AppError::internal("显示器服务不可用。"))),
        };

        if let Err(error) = &result {
            log::warn!("Command failed: {} — {}", error.code, error.message);
        }
        let _ = job.reply.send(result);
    }
    log::info!("DDC worker thread stopped");
}
