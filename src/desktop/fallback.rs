/// GNOME fallback: no system tray. Opens the settings window on startup so
/// the user has a way to interact with the app. Monitoring and notifications
/// continue running in the background after the window is closed.
use crate::config::Config;
use crate::monitor::{SharedPressure, SharedRamMb, SharedTopProcs};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use super::WindowRequest;

pub struct TrayHandle;

impl TrayHandle {
    pub fn notify(&self) {}
}

pub fn start(
    _shared_ram: SharedRamMb,
    _config: Arc<Mutex<Config>>,
    _total_ram_mb: u64,
    window_tx: Sender<WindowRequest>,
    _shared_top_procs: SharedTopProcs,
    _shared_pressure: SharedPressure,
) -> TrayHandle {
    let _ = window_tx.send(WindowRequest::Settings);
    TrayHandle
}
