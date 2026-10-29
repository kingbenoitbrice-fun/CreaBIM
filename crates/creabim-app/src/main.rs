//! CreaBIM desktop app.

mod align3d;
mod aligntool;
mod annotate;
mod app;
mod boxsel;
mod browser;
mod camera;
mod centres;
mod commands;
mod editmode;
mod files;
mod handles;
mod help;
mod hoverpick;
mod icons;
mod memberstool;
mod modal;
mod objecttool;
mod optionbar;
mod outline;
mod paneface;
mod planview;
mod render3d;
mod roomtool;
mod schedulepane;
mod selection;
mod sheetview;
mod sketch;
mod slabtool;
mod snaps;
mod textures;
mod theme;
mod ui_model;
mod units;
mod webui;

fn main() -> eframe::Result {
    #[cfg(target_os = "linux")]
    linux_setup();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("CreaBIM")
            .with_inner_size([1400.0, 860.0]),
        renderer: eframe::Renderer::Wgpu,
        #[cfg(target_os = "linux")]
        event_loop_builder: Some(Box::new(|b| {
            use winit::platform::x11::EventLoopBuilderExtX11;
            b.with_x11();
        })),
        ..Default::default()
    };
    eframe::run_native(
        "CreaBIM",
        options,
        Box::new(|cc| Ok(Box::new(app::CreaBimApp::new(cc)))),
    )
}

/// The web panes are GTK child windows of an X11 window, so on Wayland the
/// app runs through XWayland, and GTK has to be started first.
#[cfg(target_os = "linux")]
fn linux_setup() {
    // Without this WebKit draws nothing on some GPUs and in virtual displays.
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    std::env::set_var("GDK_BACKEND", "x11");
    if gtk::init().is_err() {
        // The app still runs, with the classic panels.
        std::env::set_var("CREABIM_CLASSIC_UI", "1");
    }
}
