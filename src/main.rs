mod app;
mod audio;
mod gui;
mod pedals;
mod project;

fn main() -> eframe::Result<()> {
    gui::run()
}
