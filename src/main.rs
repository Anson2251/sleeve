mod app;
mod i18n;
mod models;
mod services;
mod ui;

use relm4::{RelmApp, gtk::gio};

use app::AppModel;
use i18n::Language;

fn main() {
    i18n::init(Language::System);
    gio::resources_register_include!("icons.gresource")
        .unwrap_or_else(|_| panic!("{}", i18n::tr("app.resource_error")));

    RelmApp::new("com.github.anson2251.sleeve").run::<AppModel>(());
}
