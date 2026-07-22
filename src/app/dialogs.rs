use relm4::{
    ComponentSender,
    gtk::{
        self,
        gio::prelude::FileExt,
        prelude::{FileChooserExt, NativeDialogExt},
    },
};

use super::{AppModel, AppMsg};

pub(super) fn choose_directory(root: &gtk::Window, sender: ComponentSender<AppModel>) {
    let chooser = gtk::FileChooserNative::new(
        Some(&crate::t!("dialog.choose_music_directory")),
        Some(root),
        gtk::FileChooserAction::SelectFolder,
        Some(&crate::t!("dialog.open")),
        Some(&crate::t!("dialog.cancel")),
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
        Some(&crate::t!("dialog.choose_cover")),
        Some(root),
        gtk::FileChooserAction::Open,
        Some(&crate::t!("dialog.choose")),
        Some(&crate::t!("dialog.cancel")),
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
