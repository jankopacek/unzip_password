//! egui/eframe front-end.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};

use eframe::egui;

use crate::core::{self, MAX_PASSWORD_CHARS, Options};

enum Msg {
    Line { text: String, error: bool },
    Done(Result<(), String>),
}

struct App {
    password: String,
    show_password: bool,
    input: String,
    output: String,
    remove_zip: bool,
    log: Vec<(String, bool)>,
    worker: Option<Worker>,
}

struct Worker {
    rx: Receiver<Msg>,
    cancel: Arc<AtomicBool>,
}

impl Default for App {
    fn default() -> Self {
        let cwd = std::env::current_dir().unwrap_or_default().display().to_string();
        Self {
            password: std::env::var("RENTGENY_PASSWORD").unwrap_or_default(),
            show_password: false,
            input: cwd.clone(),
            output: cwd,
            remove_zip: false,
            log: Vec::new(),
            worker: None,
        }
    }
}

impl App {
    fn start(&mut self, ctx: &egui::Context) {
        let opts = Options {
            password: self.password.clone(),
            input: PathBuf::from(&self.input),
            output: PathBuf::from(&self.output),
            remove_zip: self.remove_zip,
        };
        self.log.clear();
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let (ctx, flag) = (ctx.clone(), cancel.clone());
        std::thread::spawn(move || {
            let result = core::run(&opts, &flag, &mut |event| {
                let _ = tx.send(Msg::Line { text: event.to_string(), error: event.is_error() });
                ctx.request_repaint();
            });
            let _ = tx.send(Msg::Done(result));
            ctx.request_repaint();
        });
        self.worker = Some(Worker { rx, cancel });
    }

    fn poll(&mut self) {
        let Some(worker) = &self.worker else { return };
        let mut finished = false;
        while let Ok(msg) = worker.rx.try_recv() {
            match msg {
                Msg::Line { text, error } => self.log.push((text, error)),
                Msg::Done(result) => {
                    if let Err(e) = result {
                        self.log.push((format!("error: {e}"), true));
                    } else {
                        self.log.push(("done".into(), false));
                    }
                    finished = true;
                }
            }
        }
        if finished {
            self.worker = None;
        }
    }
}

const LABEL_WIDTH: f32 = 130.0;
const TRAILING_WIDTH: f32 = 110.0;
const ROW_HEIGHT: f32 = 34.0;

/// One form row: fixed-width label, a text field stretched to fill all remaining
/// width, and a fixed-width trailing widget (button/checkbox) added by `trailing`.
fn form_row(
    ui: &mut egui::Ui,
    label: &str,
    field: impl FnOnce(&mut egui::Ui, f32),
    trailing: impl FnOnce(&mut egui::Ui),
) {
    ui.horizontal(|ui| {
        ui.add_sized([LABEL_WIDTH, ROW_HEIGHT], egui::Label::new(label));
        let width = (ui.available_width() - TRAILING_WIDTH - ui.spacing().item_spacing.x).max(100.0);
        field(ui, width);
        ui.allocate_ui_with_layout(
            egui::vec2(TRAILING_WIDTH, ROW_HEIGHT),
            egui::Layout::left_to_right(egui::Align::Center),
            trailing,
        );
    });
}

fn folder_row(ui: &mut egui::Ui, label: &str, value: &mut String, enabled: bool) {
    let mut picked = false;
    let dir = value.clone();
    form_row(
        ui,
        label,
        |ui, width| {
            ui.add_enabled(
                enabled,
                egui::TextEdit::singleline(value)
                    .margin(egui::vec2(8.0, 6.0))
                    .align(egui::Align2::LEFT_CENTER)
                    .min_size(egui::vec2(width, ROW_HEIGHT))
                    .desired_width(width),
            );
        },
        |ui| picked = ui.add_enabled(enabled, egui::Button::new("Browse…")).clicked(),
    );
    if picked {
        let mut dialog = rfd::FileDialog::new();
        if std::path::Path::new(&dir).is_dir() {
            dialog = dialog.set_directory(&dir);
        }
        if let Some(new_dir) = dialog.pick_folder() {
            *value = new_dir.display().to_string();
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.poll();
        let ctx = ui.ctx().clone();
        let running = self.worker.is_some();

        egui::CentralPanel::default().show(ui, |ui| {
            let hide = !self.show_password;
            form_row(
                ui,
                "Password",
                |ui, width| {
                    ui.add_enabled(
                        !running,
                        egui::TextEdit::singleline(&mut self.password)
                            .password(hide)
                            .char_limit(MAX_PASSWORD_CHARS)
                            .margin(egui::vec2(8.0, 6.0))
                            .align(egui::Align2::LEFT_CENTER)
                            .min_size(egui::vec2(width, ROW_HEIGHT))
                            .desired_width(width),
                    );
                },
                |ui| {
                    ui.checkbox(&mut self.show_password, "Show");
                },
            );
            folder_row(ui, "Input folder", &mut self.input, !running);
            folder_row(ui, "Output folder", &mut self.output, !running);

            ui.add_enabled(
                !running,
                egui::Checkbox::new(&mut self.remove_zip, "Delete each zip after successful extraction"),
            );

            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().button_padding = egui::vec2(18.0, 8.0);
                if running {
                    if ui.button("Cancel").clicked() {
                        if let Some(w) = &self.worker {
                            w.cancel.store(true, Ordering::Relaxed);
                        }
                    }
                    ui.spinner();
                } else if ui
                    .add_enabled(!self.password.is_empty(), egui::Button::new("Extract"))
                    .clicked()
                {
                    self.start(&ctx);
                }
            });

            ui.separator();
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for (line, error) in &self.log {
                        let text = egui::RichText::new(line).monospace();
                        ui.label(if *error { text.color(ui.visuals().error_fg_color) } else { text });
                    }
                });
        });
    }
}

pub fn run() -> ExitCode {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([820.0, 600.0])
            .with_min_inner_size([480.0, 360.0]),
        ..Default::default()
    };
    match eframe::run_native(
        "rentgeny",
        options,
        Box::new(|cc| {
            // Larger base sizes; Ctrl +/- (built into egui) zooms the whole UI further.
            cc.egui_ctx.all_styles_mut(|style| {
                for font in style.text_styles.values_mut() {
                    font.size *= 1.3;
                }
                style.spacing.button_padding = egui::vec2(12.0, 6.0);
                style.spacing.interact_size.y = 30.0;
            });
            Ok(Box::new(App::default()))
        }),
    ) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: cannot start GUI: {e}");
            ExitCode::FAILURE
        }
    }
}
