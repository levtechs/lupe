mod app;
mod audio;
mod content;
mod gui;
mod pedals;
mod project;

fn main() -> eframe::Result<()> {
    gui::run()
}
