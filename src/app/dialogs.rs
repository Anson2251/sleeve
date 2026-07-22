use relm4::{
    ComponentSender,
    gtk::{
        self,
        gio::prelude::FileExt,
        prelude::{EditableExt, FileChooserExt, NativeDialogExt},
    },
};

use super::{AppModel, AppMsg};

pub(super) fn choose_directory(root: &gtk::Window, sender: ComponentSender<AppModel>) {
    let chooser = gtk::FileChooserNative::new(
        Some("选择音乐目录"),
        Some(root),
        gtk::FileChooserAction::SelectFolder,
        Some("打开"),
        Some("取消"),
    );
    chooser.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept
            && let Some(path) = dialog.file().and_then(|file| file.path())
        {
            sender.input(AppMsg::DirectoryChosen(path));
        }
        dialog.destroy();
    });
    chooser.show();
}

pub(super) fn choose_cover(root: &gtk::Window, sender: ComponentSender<AppModel>) {
    let chooser = gtk::FileChooserNative::new(
        Some("选择封面图片"),
        Some(root),
        gtk::FileChooserAction::Open,
        Some("选择"),
        Some("取消"),
    );
    chooser.connect_response(move |dialog, response| {
        if response == gtk::ResponseType::Accept
            && let Some(path) = dialog.file().and_then(|file| file.path())
        {
            sender.input(AppMsg::CoverChosen(path));
        }
        dialog.destroy();
    });
    chooser.show();
}

pub(super) fn sync_entry(entry: &gtk::Entry, value: &str) {
    if entry.text().as_str() != value {
        entry.set_text(value);
    }
}
