use crate::config::Config;
use crate::monitor::{SharedPressure, SharedRamMb, SharedTopProcs, pressure_from_shared};
use ksni::menu::*;
use ksni::Icon;
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

struct RamboTray {
    shared_ram: SharedRamMb,
    config: Arc<Mutex<Config>>,
    total_ram_mb: u64,
    settings_tx: Sender<()>,
    shared_top_procs: SharedTopProcs,
    shared_pressure: SharedPressure,
}

/// Generates a 22×22 ARGB32 bar-gauge icon representing free RAM.
///
/// The bar fills from the bottom proportional to `free_mb / total_mb` and
/// is coloured green (healthy), amber (approaching threshold), or red (critical).
fn make_ram_icon(free_mb: u64, total_mb: u64, threshold_mb: u64) -> Vec<Icon> {
    const W: usize = 22;
    const H: usize = 22;
    let mut data = vec![0u8; W * H * 4];

    let (r, g, b): (u8, u8, u8) = if total_mb == 0 || free_mb == 0 {
        (128, 128, 128) // gray – not yet loaded
    } else if free_mb < threshold_mb {
        (220, 50, 50) // red – critical
    } else if free_mb < threshold_mb * 2 {
        (230, 160, 0) // amber – low
    } else {
        (60, 180, 60) // green – healthy
    };

    let ratio = if total_mb > 0 {
        (free_mb as f64 / total_mb as f64).clamp(0.0, 1.0)
    } else {
        0.5
    };

    // Inner bar area: 2 px border on each side → 18 rows of fill
    let inner_h = H - 4;
    let fill_height = (inner_h as f64 * ratio).round() as usize;
    let fill_start_y = H - 2 - fill_height;

    for y in 0..H {
        for x in 0..W {
            let idx = (y * W + x) * 4;
            let border = x < 2 || x >= W - 2 || y < 2 || y >= H - 2;
            let filled = !border && y >= fill_start_y;

            // ARGB32 network byte order: [A, R, G, B]
            let (a, pr, pg, pb): (u8, u8, u8, u8) = if border {
                (255, 80, 80, 80)
            } else if filled {
                (255, r, g, b)
            } else {
                (180, 40, 40, 40)
            };
            data[idx] = a;
            data[idx + 1] = pr;
            data[idx + 2] = pg;
            data[idx + 3] = pb;
        }
    }

    vec![Icon {
        width: W as i32,
        height: H as i32,
        data,
    }]
}

impl ksni::Tray for RamboTray {
    fn icon_name(&self) -> String {
        // Return empty so the DE uses icon_pixmap instead of a named system icon
        String::new()
    }

    fn icon_pixmap(&self) -> Vec<Icon> {
        let free_mb = self.shared_ram.load(Ordering::Relaxed);
        let threshold = self.config.lock().unwrap().threshold_mb;
        make_ram_icon(free_mb, self.total_ram_mb, threshold)
    }

    fn title(&self) -> String {
        "rambo".into()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let free_mb = self.shared_ram.load(Ordering::Relaxed);
        let pressure = pressure_from_shared(&self.shared_pressure);
        let top_procs = self.shared_top_procs.lock().unwrap().clone();

        let mut desc = format!("Free RAM: {free_mb} MB | Pressure: {pressure:.1}%\n\nTop processes:");
        for (i, (name, bytes)) in top_procs.iter().enumerate() {
            let mb = *bytes as f64 / 1_048_576.0;
            desc.push_str(&format!("\n{}. {} — {:.0} MB", i + 1, name, mb));
        }

        ksni::ToolTip {
            icon_name: String::new(),
            icon_pixmap: vec![],
            title: "rambo".into(),
            description: desc,
        }
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let free_mb = self.shared_ram.load(Ordering::Relaxed);
        let threshold = self.config.lock().unwrap().threshold_mb;

        let status_label = if free_mb == 0 {
            "RAM: loading…".to_string()
        } else if free_mb < threshold {
            format!("⚠ Free RAM: {free_mb} MB (LOW)")
        } else {
            format!("✓ Free RAM: {free_mb} MB")
        };

        const PRESETS: [u64; 5] = [256, 512, 1024, 2048, 4096];
        let threshold_items: Vec<MenuItem<Self>> = PRESETS
            .iter()
            .map(|&preset| {
                CheckmarkItem {
                    label: format!("{preset} MB"),
                    checked: threshold == preset,
                    activate: Box::new(move |this: &mut RamboTray| {
                        let mut c = this.config.lock().unwrap();
                        c.threshold_mb = preset;
                        let _ = c.save();
                    }),
                    ..Default::default()
                }
                .into()
            })
            .collect();

        vec![
            StandardItem {
                label: status_label,
                enabled: false,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: format!("Set Threshold ({threshold} MB)…"),
                submenu: threshold_items,
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Settings…".into(),
                icon_name: "preferences-system".into(),
                activate: Box::new(|this: &mut RamboTray| {
                    let _ = this.settings_tx.send(());
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|_this: &mut RamboTray| std::process::exit(0)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Opaque handle to the tray service. Call `notify()` to push a property refresh.
pub struct TrayHandle(ksni::Handle<RamboTray>);

impl TrayHandle {
    pub fn notify(&self) {
        self.0.update(|_| {});
    }
}

pub fn start(
    shared_ram: SharedRamMb,
    config: Arc<Mutex<Config>>,
    total_ram_mb: u64,
    settings_tx: Sender<()>,
    shared_top_procs: SharedTopProcs,
    shared_pressure: SharedPressure,
) -> TrayHandle {
    let service = ksni::TrayService::new(RamboTray {
        shared_ram,
        config,
        total_ram_mb,
        settings_tx,
        shared_top_procs,
        shared_pressure,
    });
    let handle = service.handle();
    service.spawn();
    TrayHandle(handle)
}
