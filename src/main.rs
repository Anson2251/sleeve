mod app;
mod models;
mod services;
mod ui;

use relm4::RelmApp;

use app::AppModel;

fn main() {
    RelmApp::new("com.anson.sleeve").run::<AppModel>(());
}
