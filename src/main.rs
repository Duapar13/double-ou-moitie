mod app;
mod live;
mod sim;
mod stats;
mod theory;
mod tree;
mod util;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Double ou Moitié — simulateur")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1100.0, 700.0])
            // sans ça, eframe remplace l'icône du Dock par le logo egui
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png")).expect("icône PNG valide"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "double-ou-moitie",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}
