use crate::config::{Config, KillableApp};
use eframe::egui;
use egui::{CentralPanel, ScrollArea};
use egui_dnd::dnd;
use std::sync::{Arc, Mutex};

/// Opens the settings window. Blocks until the window is closed.
/// Must be called from the main thread (winit/eframe requirement).
pub fn open_blocking(config: Arc<Mutex<Config>>) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rambo — Settings")
            .with_inner_size([480.0, 600.0])
            .with_resizable(true),
        ..Default::default()
    };

    let new_name = Arc::new(Mutex::new(String::new()));
    let new_display = Arc::new(Mutex::new(String::new()));

    let _ = eframe::run_simple_native("rambo-settings", options, move |ctx, _frame| {
        ctx.set_pixels_per_point(1.2);

        CentralPanel::default().show(ctx, |ui| {
            ScrollArea::vertical().show(ui, |ui| {
                ui.add_space(8.0);
                ui.heading("⚙ Rambo Settings");
                ui.add_space(12.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("General").strong());
                    ui.add_space(4.0);
                    egui::Grid::new("general_grid")
                        .num_columns(2)
                        .spacing([16.0, 8.0])
                        .show(ui, |ui| {
                            let mut cfg = config.lock().unwrap();
                            ui.label("RAM threshold (MB)");
                            if ui.add(egui::DragValue::new(&mut cfg.threshold_mb).range(64..=65536)).changed() {
                                let _ = cfg.save();
                            }
                            ui.end_row();
                            ui.label("Countdown (seconds)");
                            if ui.add(egui::DragValue::new(&mut cfg.countdown_seconds).range(5..=300)).changed() {
                                let _ = cfg.save();
                            }
                            ui.end_row();
                            ui.label("Check interval (seconds)");
                            if ui.add(egui::DragValue::new(&mut cfg.check_interval_seconds).range(1..=60)).changed() {
                                let _ = cfg.save();
                            }
                            ui.end_row();
                            ui.label("Snooze duration (seconds)");
                            if ui.add(egui::DragValue::new(&mut cfg.snooze_seconds).range(30..=3600)).changed() {
                                let _ = cfg.save();
                            }
                            ui.end_row();
                        });
                });

                ui.add_space(12.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("Kill Priority").strong());
                    ui.label(
                        egui::RichText::new("Drag ≡ to reorder. First enabled running app is targeted.")
                            .small()
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(6.0);

                    let mut apps = config.lock().unwrap().killable_apps.clone();
                    let mut changed = false;
                    let mut to_remove: Option<usize> = None;

                    let mut row = 0usize;
                    let dnd_response = dnd(ui, "kill_priority_dnd").show_vec(&mut apps, |ui, app, handle, state| {
                        let current = row;
                        row += 1;
                        ui.horizontal(|ui| {
                            handle.ui(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("≡")
                                        .color(if state.dragged {
                                            ui.visuals().strong_text_color()
                                        } else {
                                            ui.visuals().weak_text_color()
                                        }),
                                );
                            });
                            changed |= ui.checkbox(&mut app.enabled, "").changed();
                            ui.label(app.label());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("🗑").on_hover_text("Remove").clicked() {
                                    to_remove = Some(current);
                                    changed = true;
                                }
                            });
                        });
                    });

                    // Persist after a completed drag-and-drop reorder
                    if dnd_response.is_drag_finished() {
                        changed = true;
                    }

                    if let Some(idx) = to_remove {
                        apps.remove(idx);
                    }

                    if changed {
                        let mut cfg = config.lock().unwrap();
                        cfg.killable_apps = apps;
                        let _ = cfg.save();
                    }
                });

                ui.add_space(12.0);

                ui.group(|ui| {
                    ui.label(egui::RichText::new("Add App").strong());
                    ui.add_space(4.0);

                    let mut name = new_name.lock().unwrap();
                    let mut display = new_display.lock().unwrap();

                    egui::Grid::new("add_app_grid")
                        .num_columns(2)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            ui.label("Process name");
                            ui.add(egui::TextEdit::singleline(&mut *name).hint_text("e.g. spotify"));
                            ui.end_row();
                            ui.label("Display name");
                            ui.add(egui::TextEdit::singleline(&mut *display).hint_text("e.g. Spotify (optional)"));
                            ui.end_row();
                        });

                    ui.add_space(4.0);

                    let can_add = !name.trim().is_empty();
                    if ui.add_enabled(can_add, egui::Button::new("Add")).clicked() {
                        let display_name = if display.trim().is_empty() {
                            None
                        } else {
                            Some(display.trim().to_owned())
                        };
                        let new_app = KillableApp {
                            name: name.trim().to_lowercase(),
                            display_name,
                            enabled: true,
                        };
                        let mut cfg = config.lock().unwrap();
                        cfg.killable_apps.push(new_app);
                        let _ = cfg.save();
                        drop(cfg);
                        name.clear();
                        display.clear();
                    }
                });

                ui.add_space(8.0);
            });
        });
    });
}
