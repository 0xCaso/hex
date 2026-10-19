//! Launch at login. macOS registers the app with `SMAppService`; Windows
//! writes the current user's `Run` registry value. Linux starts the
//! `hex.service` unit from its install script or Nix module, so Hex does not
//! offer the setting there.

use color_eyre::eyre::Result;
#[cfg(target_os = "macos")]
use color_eyre::eyre::eyre;
#[cfg(target_os = "macos")]
use objc2_service_management::{SMAppService, SMAppServiceStatus};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoginItemStatus {
    Disabled,
    Enabled,
    RequiresApproval,
}

#[cfg(target_os = "macos")]
fn status() -> Result<LoginItemStatus> {
    // SAFETY: mainAppService and status do not retain caller-owned pointers.
    let status = unsafe { SMAppService::mainAppService().status() };
    match status {
        SMAppServiceStatus::NotRegistered | SMAppServiceStatus::NotFound => {
            Ok(LoginItemStatus::Disabled)
        }
        SMAppServiceStatus::Enabled => Ok(LoginItemStatus::Enabled),
        SMAppServiceStatus::RequiresApproval => Ok(LoginItemStatus::RequiresApproval),
        _ => Err(eyre!("macOS returned an unknown login item status")),
    }
}

#[cfg(target_os = "macos")]
fn set_enabled(enabled: bool) -> Result<LoginItemStatus> {
    // SAFETY: SMAppService owns the NSError produced by these synchronous APIs.
    let service = unsafe { SMAppService::mainAppService() };
    let current = status()?;
    if enabled && current == LoginItemStatus::Disabled {
        unsafe { service.registerAndReturnError() }.map_err(|error| eyre!(error.to_string()))?;
    } else if !enabled && current != LoginItemStatus::Disabled {
        unsafe { service.unregisterAndReturnError() }.map_err(|error| eyre!(error.to_string()))?;
    }
    status()
}

#[cfg(target_os = "macos")]
fn open_settings() {
    // SAFETY: This asks macOS to open its Login Items settings pane.
    unsafe { SMAppService::openSystemSettingsLoginItems() };
}

#[cfg(target_os = "windows")]
fn status() -> Result<LoginItemStatus> {
    Ok(if crate::windows_login_item::is_enabled()? {
        LoginItemStatus::Enabled
    } else {
        LoginItemStatus::Disabled
    })
}

#[cfg(target_os = "windows")]
fn set_enabled(enabled: bool) -> Result<LoginItemStatus> {
    crate::windows_login_item::set_enabled(enabled)?;
    status()
}

/// Windows never asks for approval, so there is no settings page to open.
#[cfg(target_os = "windows")]
fn open_settings() {}

#[cfg(target_os = "linux")]
fn status() -> Result<LoginItemStatus> {
    Err(color_eyre::eyre::eyre!(
        "hex.service decides whether Hex starts at login"
    ))
}

#[cfg(target_os = "linux")]
fn set_enabled(_: bool) -> Result<LoginItemStatus> {
    status()
}

#[cfg(target_os = "linux")]
fn open_settings() {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginItemRequest {
    Status,
    SetEnabled(bool),
    OpenSettings,
}

pub struct LoginItemFailure {
    pub status: Option<LoginItemStatus>,
    pub message: String,
}

pub struct LoginItemResponse {
    pub request: LoginItemRequest,
    pub result: Result<LoginItemStatus, LoginItemFailure>,
}

/// One worker per Settings window. Polling never queues behind an active request;
/// user actions replace the single pending action and run before the next poll.
pub struct LoginItemWorker {
    requests: std::sync::mpsc::SyncSender<LoginItemRequest>,
    responses: std::sync::mpsc::Receiver<Result<LoginItemStatus, LoginItemFailure>>,
    active: Option<LoginItemRequest>,
    pending: Option<LoginItemRequest>,
}

impl LoginItemWorker {
    pub fn new() -> Result<Self, String> {
        Self::with_handler(|request| {
            let handle = || {
                let result = match request {
                    LoginItemRequest::Status => status(),
                    LoginItemRequest::SetEnabled(enabled) => set_enabled(enabled),
                    LoginItemRequest::OpenSettings => {
                        open_settings();
                        status()
                    }
                };
                result.map_err(|error| LoginItemFailure {
                    // Registration may change the OS state even when it reports an error.
                    status: matches!(request, LoginItemRequest::SetEnabled(_))
                        .then(|| status().ok())
                        .flatten(),
                    message: error.to_string(),
                })
            };
            #[cfg(target_os = "macos")]
            let result = objc2::rc::autoreleasepool(|_| handle());
            #[cfg(not(target_os = "macos"))]
            let result = handle();
            result
        })
    }

    fn with_handler(
        mut handle: impl FnMut(LoginItemRequest) -> Result<LoginItemStatus, LoginItemFailure>
        + Send
        + 'static,
    ) -> Result<Self, String> {
        let (requests, receiver) = std::sync::mpsc::sync_channel(1);
        let (sender, responses) = std::sync::mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("login-item".into())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    if sender.send(handle(request)).is_err() {
                        break;
                    }
                }
            })
            .map_err(|error| format!("Could not start Launch at Login worker: {error}"))?;
        let mut worker = Self {
            requests,
            responses,
            active: None,
            pending: None,
        };
        worker.request(LoginItemRequest::Status)?;
        Ok(worker)
    }

    pub fn request(&mut self, request: LoginItemRequest) -> Result<(), String> {
        if self.active.is_some() {
            if request != LoginItemRequest::Status {
                self.pending = Some(request);
            }
            return Ok(());
        }
        self.requests
            .try_send(request)
            .map_err(|error| format!("Could not contact Launch at Login worker: {error}"))?;
        self.active = Some(request);
        Ok(())
    }

    pub fn poll(&mut self) -> Result<Option<LoginItemResponse>, String> {
        let result = match self.responses.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return Ok(None),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                return Err("Launch at Login worker stopped unexpectedly".into());
            }
        };
        let request = self.active.take().expect("response has an active request");
        if let Some(pending) = self.pending.take() {
            self.request(pending)?;
        }
        Ok(Some(LoginItemResponse { request, result }))
    }

    pub fn desired_enabled(&self) -> Option<bool> {
        match self.pending.or(self.active) {
            Some(LoginItemRequest::SetEnabled(enabled)) => Some(enabled),
            _ => None,
        }
    }
}

// Dropping the channels lets the worker exit after its current native call.
// Never join here: ServiceManagement can block, including while Settings closes.

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{RecvTimeoutError, channel};
    use std::time::{Duration, Instant};

    fn completed(worker: &mut LoginItemWorker) -> LoginItemResponse {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(response) = worker.poll().unwrap() {
                return response;
            }
            assert!(Instant::now() < deadline, "worker did not complete");
            std::thread::yield_now();
        }
    }

    #[test]
    fn login_item_worker_reuses_thread_and_coalesces_actions_before_polling() {
        let (calls, observed) = channel();
        let (release, gate) = channel();
        let mut worker = LoginItemWorker::with_handler(move |request| {
            calls.send((request, std::thread::current().id())).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(LoginItemStatus::Disabled)
        })
        .unwrap();
        let (request, thread) = observed.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(request, LoginItemRequest::Status);
        assert_ne!(thread, std::thread::current().id());
        // A stalled native call neither blocks polling nor admits more status work.
        assert!(worker.poll().unwrap().is_none());
        worker.request(LoginItemRequest::SetEnabled(true)).unwrap();
        worker.request(LoginItemRequest::SetEnabled(false)).unwrap();
        for _ in 0..10 {
            worker.request(LoginItemRequest::Status).unwrap();
        }
        assert_eq!(worker.desired_enabled(), Some(false));
        release.send(()).unwrap();
        assert_eq!(completed(&mut worker).request, LoginItemRequest::Status);
        assert_eq!(
            observed.recv_timeout(Duration::from_secs(5)).unwrap(),
            (LoginItemRequest::SetEnabled(false), thread)
        );
        release.send(()).unwrap();
        assert_eq!(
            completed(&mut worker).request,
            LoginItemRequest::SetEnabled(false)
        );
        worker.request(LoginItemRequest::OpenSettings).unwrap();
        assert_eq!(
            observed.recv_timeout(Duration::from_secs(5)).unwrap(),
            (LoginItemRequest::OpenSettings, thread)
        );
        release.send(()).unwrap();
        completed(&mut worker);
        drop(worker);
        assert_eq!(
            observed.recv_timeout(Duration::from_secs(5)),
            Err(RecvTimeoutError::Disconnected)
        );
    }

    #[test]
    fn login_item_worker_closes_without_waiting_for_native_call() {
        let (calls, observed) = channel();
        let (release, gate) = channel();
        let mut worker = LoginItemWorker::with_handler(move |request| {
            calls.send(request).unwrap();
            gate.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(LoginItemStatus::Disabled)
        })
        .unwrap();
        assert_eq!(
            observed.recv_timeout(Duration::from_secs(5)).unwrap(),
            LoginItemRequest::Status
        );
        worker.request(LoginItemRequest::SetEnabled(true)).unwrap();
        drop(worker);
        // The call is still blocked when the owner is dropped. Closing must not join it
        // or execute the user action that was still pending in the closed window.
        release.send(()).unwrap();
        assert_eq!(
            observed.recv_timeout(Duration::from_secs(5)),
            Err(RecvTimeoutError::Disconnected)
        );
    }
}
