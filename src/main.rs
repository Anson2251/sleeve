mod app;
mod models;
mod services;
mod ui;

use relm4::{RelmApp, gtk::gio};

use app::AppModel;

fn main() {
    gio::resources_register_include!("icons.gresource").expect("无法注册内嵌图标资源");

    RelmApp::new("com.github.anson2251.sleeve").run::<AppModel>(());
}
