//! CreaBIM desktop app.

mod app;
mod boxsel;
mod camera;
mod files;
mod handles;
mod icons;
mod modal;
mod outline;
mod planview;
mod render3d;
mod schedulepane;
mod selection;
mod sheetview;
mod sketch;
mod slabtool;
mod snaps;
mod textures;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("CreaBIM")
            .with_inner_size([1400.0, 860.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "CreaBIM",
        options,
        Box::new(|cc| Ok(Box::new(app::CreaBimApp::new(cc)))),
    )
}
