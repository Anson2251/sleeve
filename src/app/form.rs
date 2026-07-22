use std::{cell::Cell, collections::HashSet, rc::Rc};

use relm4::{
    ComponentParts, ComponentSender, RelmWidgetExt, SimpleComponent,
    gtk::{self, prelude::*},
};

use crate::models::{TagDraft, TagField};

#[derive(Debug, Clone)]
pub(super) struct FormState {
    draft: TagDraft,
    mixed_fields: HashSet<TagField>,
    pub(super) visible: bool,
    pub(super) is_sensitive: bool,
    pub(super) is_batch_editing: bool,
}

impl FormState {
    pub(super) fn from_draft(
        draft: &TagDraft,
        mixed_fields: &HashSet<TagField>,
        visible: bool,
        is_sensitive: bool,
        is_batch_editing: bool,
    ) -> Self {
        Self {
            draft: draft.clone(),
            mixed_fields: mixed_fields.clone(),
            visible,
            is_sensitive,
            is_batch_editing,
        }
    }

    pub(super) fn placeholder(&self, field: TagField) -> String {
        if self.mixed_fields.contains(&field) {
            crate::t!("form.multiple_values")
        } else {
            String::new()
        }
    }

    pub(super) fn value(&self, field: TagField) -> &str {
        self.draft.value(field)
    }

    pub(super) fn validation_error(&self, field: TagField) -> Option<String> {
        self.draft.validation_error(field)
    }
}

#[derive(Debug)]
pub(super) enum FormInput {
    SetState(FormState),
}

#[derive(Debug)]
pub(super) enum FormOutput {
    SetField(TagField, String),
}

pub(super) struct FormComponent {
    state: FormState,
}

#[relm4::component(pub(super))]
impl SimpleComponent for FormComponent {
    type Init = FormState;
    type Input = FormInput;
    type Output = FormOutput;

    additional_fields! {
        syncing: Rc<Cell<bool>>,
    }

    view! {
        #[name = "form"]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 8,
            set_hexpand: true,
            set_width_request: 480,
            set_margin_all: 20,
            #[watch]
            set_visible: model.state.visible,
            #[watch]
            set_sensitive: model.state.is_sensitive,
            gtk::Label {
                set_label: &crate::t!("form.batch_hint"),
                set_halign: gtk::Align::Start,
                add_css_class: "dim-label",
                #[watch]
                set_visible: model.state.is_batch_editing
            },
            gtk::Label {
                set_label: " ",
                #[watch]
                set_visible:
                !model.state.is_batch_editing
            },
            gtk::Label {
                set_label: &crate::t!("form.title"),
                set_halign: gtk::Align::Start
            },
            #[name = "title"]
            gtk::Entry {
                #[watch]
                set_placeholder_text: Some(&model.state.placeholder(TagField::Title)),
                connect_changed[sender, syncing] => move |entry| if !syncing.get() {
                    let _ = sender.output(FormOutput::SetField(TagField::Title, entry.text().to_string()));
                }
            },
            gtk::Label {
                #[watch]
                set_label: &model.state.validation_error(TagField::Title).unwrap_or_default(),
                #[watch] set_visible: model.state.validation_error(TagField::Title).is_some(),
                add_css_class: "error",
                set_halign: gtk::Align::Start
            },
            gtk::Label {
                set_label: &crate::t!("form.artist"),
                set_halign: gtk::Align::Start
            },
            #[name = "artist"]
            gtk::Entry {
                #[watch]
                set_placeholder_text: Some(&model.state.placeholder(TagField::Artist)),
                connect_changed[sender, syncing] => move |entry| if !syncing.get() {
                    let _ = sender.output(FormOutput::SetField(TagField::Artist, entry.text().to_string()));
                }
            },
            gtk::Label {
                #[watch]
                set_label: &model.state.validation_error(TagField::Artist).unwrap_or_default(),
                #[watch]
                set_visible: model.state.validation_error(TagField::Artist).is_some(),
                add_css_class: "error",
                set_halign: gtk::Align::Start
            },
            gtk::Label {
                set_label: &crate::t!("form.album"),
                set_halign: gtk::Align::Start
            },
            #[name = "album"]
            gtk::Entry {
                #[watch]
                set_placeholder_text: Some(&model.state.placeholder(TagField::Album)),
                connect_changed[sender, syncing] => move |entry| if !syncing.get() {
                    let _ = sender.output(FormOutput::SetField(TagField::Album, entry.text().to_string()));
                }
            },
            gtk::Label {
                #[watch]
                set_label: &model.state.validation_error(TagField::Album).unwrap_or_default(),
                #[watch]
                set_visible: model.state.validation_error(TagField::Album).is_some(),
                add_css_class: "error",
                set_halign: gtk::Align::Start
            },
            gtk::Label {
                set_label: &crate::t!("form.album_artist"),
                set_halign: gtk::Align::Start
            },
            #[name = "album_artist"]
            gtk::Entry {
                #[watch]
                set_placeholder_text: Some(&model.state.placeholder(TagField::AlbumArtist)),
                connect_changed[sender, syncing] => move |entry| if !syncing.get() {
                    let _ = sender.output(FormOutput::SetField(TagField::AlbumArtist, entry.text().to_string()));
                }
            },
            gtk::Label {
                #[watch]
                set_label: &model.state.validation_error(TagField::AlbumArtist).unwrap_or_default(),
                #[watch]
                set_visible: model.state.validation_error(TagField::AlbumArtist).is_some(),
                add_css_class: "error",
                set_halign: gtk::Align::Start
            },
            gtk::Label {
                set_label: &crate::t!("form.year"),
                set_halign: gtk::Align::Start
            },
            #[name = "year"]
            gtk::Entry {
                #[watch]
                set_placeholder_text: Some(&model.state.placeholder(TagField::Year)),
                connect_changed[sender, syncing] => move |entry| if !syncing.get() {
                    let _ = sender.output(FormOutput::SetField(TagField::Year, entry.text().to_string()));
                }
            },
            gtk::Label {
                #[watch]
                set_label: &model.state.validation_error(TagField::Year).unwrap_or_default(),
                #[watch]
                set_visible: model.state.validation_error(TagField::Year).is_some(),
                add_css_class: "error",
                set_halign: gtk::Align::Start
            },
            gtk::Label {
                set_label: &crate::t!("form.track_number"),
                set_halign: gtk::Align::Start
            },
            #[name = "track"]
            gtk::Entry {
                #[watch]
                set_placeholder_text: Some(&model.state.placeholder(TagField::TrackNumber)),
                connect_changed[sender, syncing] => move |entry| if !syncing.get() {
                    let _ = sender.output(FormOutput::SetField(TagField::TrackNumber, entry.text().to_string()));
                }
            },
            gtk::Label {
                #[watch]
                set_label: &model.state.validation_error(TagField::TrackNumber).unwrap_or_default(),
                #[watch]
                set_visible: model.state.validation_error(TagField::TrackNumber).is_some(),
                add_css_class: "error",
                set_halign: gtk::Align::Start
            },
            gtk::Label {
                set_label: &crate::t!("form.disc_number"),
                set_halign: gtk::Align::Start
            },
            #[name = "disc"]
            gtk::Entry {
                #[watch]
                set_placeholder_text: Some(&model.state.placeholder(TagField::DiscNumber)),
                connect_changed[sender, syncing] => move |entry| if !syncing.get() {
                    let _ = sender.output(FormOutput::SetField(TagField::DiscNumber, entry.text().to_string()));
                }
            },
            gtk::Label {
                #[watch]
                set_label: &model.state.validation_error(TagField::DiscNumber).unwrap_or_default(),
                #[watch]
                set_visible: model.state.validation_error(TagField::DiscNumber).is_some(),
                add_css_class: "error",
                set_halign: gtk::Align::Start
            },
            gtk::Label {
                set_label: &crate::t!("form.genre"),
                set_halign: gtk::Align::Start
            },
            #[name = "genre"]
            gtk::Entry {
                #[watch]
                set_placeholder_text: Some(&model.state.placeholder(TagField::Genre)),
                connect_changed[sender, syncing] => move |entry| if !syncing.get() {
                    let _ = sender.output(FormOutput::SetField(TagField::Genre, entry.text().to_string()));
                }
            },
            gtk::Label {
                #[watch]
                set_label: &model.state.validation_error(TagField::Genre).unwrap_or_default(),
                #[watch]
                set_visible: model.state.validation_error(TagField::Genre).is_some(),
                add_css_class: "error",
                set_halign: gtk::Align::Start
            },
        }
    }

    fn init(
        state: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self { state };
        let syncing = Rc::new(Cell::new(false));
        let _ = &sender;
        let widgets = view_output!();
        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>) {
        match input {
            FormInput::SetState(state) => self.state = state,
        }
    }

    fn post_view() {
        syncing.set(true);
        sync_entry(title, model.state.value(TagField::Title));
        sync_entry(artist, model.state.value(TagField::Artist));
        sync_entry(album, model.state.value(TagField::Album));
        sync_entry(album_artist, model.state.value(TagField::AlbumArtist));
        sync_entry(year, model.state.value(TagField::Year));
        sync_entry(track, model.state.value(TagField::TrackNumber));
        sync_entry(disc, model.state.value(TagField::DiscNumber));
        sync_entry(genre, model.state.value(TagField::Genre));
        syncing.set(false);
    }
}

fn sync_entry(entry: &gtk::Entry, value: &str) {
    if entry.text().as_str() != value {
        entry.set_text(value);
    }
}
