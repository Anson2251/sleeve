use relm4::gtk::{self, prelude::*};

pub fn labeled_entry(label: &str) -> (gtk::Box, gtk::Entry, gtk::Label) {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .build();
    row.append(
        &gtk::Label::builder()
            .label(label)
            .halign(gtk::Align::Start)
            .build(),
    );
    let entry = gtk::Entry::new();
    row.append(&entry);
    let error = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .css_classes(["error"])
        .build();
    row.append(&error);
    (row, entry, error)
}
