mod menu;
mod meters;
mod session;
mod theme;
mod util;

use eframe::egui;

use crate::app::App;

pub struct LupeGui {
    app: App,
    maximize_frames_remaining: u8,
}

impl LupeGui {
    fn new() -> anyhow::Result<Self> {
        Ok(Self {
            app: App::new()?,
            maximize_frames_remaining: 3,
        })
    }
}

impl eframe::App for LupeGui {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.maximize_frames_remaining > 0 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(true));
            ctx.request_repaint();
            self.maximize_frames_remaining -= 1;
        }
        self.app.refresh();
        match self.app.screen {
            crate::app::Screen::MainMenu => menu::show(ctx, &mut self.app),
            crate::app::Screen::Session => session::show(ctx, &mut self.app),
        }
    }
}

pub fn run() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_title("lupe"),
        ..Default::default()
    };

    eframe::run_native(
        "lupe",
        native_options,
        Box::new(|cc| {
            theme::apply(&cc.egui_ctx);
            let app = LupeGui::new().map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { format!("{e:#}").into() })?;
            Ok(Box::new(app) as Box<dyn eframe::App>)
        }),
    )
}
