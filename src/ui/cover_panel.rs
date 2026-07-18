use relm4::gtk::{self, prelude::*};

pub fn metadata_row(label: &str) -> (gtk::Box, gtk::Label) {
    let row = gtk::Box::builder().spacing(8).build();
    row.append(
        &gtk::Label::builder()
            .label(label)
            .hexpand(true)
            .halign(gtk::Align::Start)
            .build(),
    );
    let value = gtk::Label::builder().halign(gtk::Align::End).build();
    row.append(&value);
    (row, value)
}
