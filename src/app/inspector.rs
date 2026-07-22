use relm4::{
    ComponentParts, ComponentSender, RelmWidgetExt, SimpleComponent, adw,
    gtk::{self, gdk, gio, prelude::*},
};

use crate::models::{AudioFile, CoverDraft, TagDraft};

use super::cover::update_cover;

#[derive(Debug, Clone)]
pub(super) struct InspectorState {
    pub(super) has_selection: bool,
    pub(super) is_sensitive: bool,
    pub(super) container: String,
    pub(super) codec: String,
    pub(super) duration: String,
    pub(super) bitrate: String,
    pub(super) sample_rate: String,
    pub(super) channels: String,
    pub(super) bits_per_sample: String,
    pub(super) file_size: String,
    pub(super) cover: CoverDraft,
    pub(super) cover_hint: String,
}

impl Default for InspectorState {
    fn default() -> Self {
        Self {
            has_selection: false,
            is_sensitive: false,
            container: "—".into(),
            codec: "—".into(),
            duration: "—".into(),
            bitrate: "—".into(),
            sample_rate: "—".into(),
            channels: "—".into(),
            bits_per_sample: "—".into(),
            file_size: "—".into(),
            cover: CoverDraft::Unavailable,
            cover_hint: crate::t!("inspector.no_cover"),
        }
    }
}

impl InspectorState {
    pub(super) fn from_selection(
        file: Option<&AudioFile>,
        draft: &TagDraft,
        cover_hint: impl Into<String>,
        is_sensitive: bool,
    ) -> Self {
        let Some(file) = file else {
            return Self {
                cover_hint: cover_hint.into(),
                ..Self::default()
            };
        };
        let metadata = &file.metadata;
        Self {
            has_selection: true,
            is_sensitive,
            container: metadata.container.clone(),
            codec: metadata.codec.clone(),
            duration: metadata.duration.clone().unwrap_or_else(|| "—".into()),
            bitrate: metadata.bitrate.clone().unwrap_or_else(|| "—".into()),
            sample_rate: metadata.sample_rate.clone().unwrap_or_else(|| "—".into()),
            channels: metadata.channels.clone().unwrap_or_else(|| "—".into()),
            bits_per_sample: metadata
                .bits_per_sample
                .clone()
                .unwrap_or_else(|| "—".into()),
            file_size: metadata.file_size.clone().unwrap_or_else(|| "—".into()),
            cover: draft.cover.clone(),
            cover_hint: cover_hint.into(),
        }
    }
}

#[derive(Debug)]
pub(super) enum InspectorInput {
    SetState(InspectorState),
}

#[derive(Debug)]
pub(super) enum InspectorOutput {
    ChooseCover,
    CoverDropped(std::path::PathBuf),
    RemoveCover,
}

pub(super) struct InspectorComponent {
    state: InspectorState,
}

#[relm4::component(pub(super))]
impl SimpleComponent for InspectorComponent {
    type Init = InspectorState;
    type Input = InspectorInput;
    type Output = InspectorOutput;

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_width_request: 310,
            set_margin_all: 16,
            #[watch]
            set_sensitive: model.state.has_selection && model.state.is_sensitive,
            gtk::Label {
                set_label: &crate::t!("inspector.title"),
                set_halign: gtk::Align::Start,
                add_css_class: "title-4"
            },
            gtk::Box {
                set_spacing: 8,
                gtk::Label {
                    set_label: &crate::t!("inspector.container"),
                    set_hexpand: true,
                    set_halign: gtk::Align::Start
                },
                gtk::Label {
                    #[watch]
                    set_label: &model.state.container,
                    set_halign: gtk::Align::End
                }
            },
            gtk::Box {
                set_spacing: 8,
                gtk::Label {
                    set_label: &crate::t!("inspector.codec"),
                    set_hexpand: true,
                    set_halign: gtk::Align::Start
                },
                gtk::Label {
                    #[watch]
                    set_label: &model.state.codec,
                    set_halign: gtk::Align::End
                }
            },
            gtk::Box {
                set_spacing: 8,
                gtk::Label {
                    set_label: &crate::t!("inspector.duration"),
                    set_hexpand: true,
                    set_halign: gtk::Align::Start
                },
                gtk::Label {
                    #[watch]
                    set_label: &model.state.duration,
                    set_halign: gtk::Align::End
                }
            },
            gtk::Box {
                set_spacing: 8,
                gtk::Label {
                    set_label: &crate::t!("inspector.bitrate"),
                    set_hexpand: true, set_halign: gtk::Align::Start
                },
                gtk::Label {
                    #[watch]
                    set_label: &model.state.bitrate,
                    set_halign: gtk::Align::End
                }
            },
            gtk::Box {
                set_spacing: 8,
                gtk::Label {
                    set_label: &crate::t!("inspector.sample_rate"),
                    set_hexpand: true,
                    set_halign: gtk::Align::Start
                },
                gtk::Label {
                    #[watch]
                    set_label: &model.state.sample_rate,
                    set_halign: gtk::Align::End
                }
            },
            gtk::Box {
                set_spacing: 8,
                gtk::Label {
                    set_label: &crate::t!("inspector.channels"),
                    set_hexpand: true,
                    set_halign: gtk::Align::Start
                },
                gtk::Label {
                    #[watch]
                    set_label: &model.state.channels,
                    set_halign: gtk::Align::End
                }
            },
            gtk::Box {
                set_spacing: 8,
                gtk::Label {
                    set_label: &crate::t!("inspector.bit_depth"),
                    set_hexpand: true,
                    set_halign: gtk::Align::Start
                },
                gtk::Label {
                    #[watch]
                    set_label: &model.state.bits_per_sample,
                    set_halign: gtk::Align::End
                }
            },
            gtk::Box {
                set_spacing: 8,
                gtk::Label {
                    set_label: &crate::t!("inspector.file_size"),
                    set_hexpand: true,
                    set_halign: gtk::Align::Start
                },
                gtk::Label {
                    #[watch]
                    set_label: &model.state.file_size,
                    set_halign: gtk::Align::End
                }
            },
            #[name = "cover_frame"]
            adw::Clamp {
                set_maximum_size: 260,
                set_tightening_threshold: 260,
                set_halign: gtk::Align::Center,
                #[wrap(Some)]
                set_child = &gtk::Frame {
                    #[name = "cover"]
                    gtk::Picture {
                        set_width_request: 260,
                        set_height_request: 260,
                        set_can_shrink: true,
                    },
                },
            },
            #[name = "cover_dimensions"]
            gtk::Label {
                set_halign: gtk::Align::Center,
                add_css_class: "dim-label",
            },
            gtk::Label {
                #[watch]
                set_label: &model.state.cover_hint,
                set_wrap: true,
                set_justify: gtk::Justification::Center,
            },
            gtk::Box {
                set_spacing: 8,
                set_halign: gtk::Align::Center,
                gtk::Button {
                    set_label: &crate::t!("inspector.choose_image"),
                    connect_clicked[sender] => move |_| {
                        let _ = sender.output(InspectorOutput::ChooseCover);
                    },
                },
                gtk::Button {
                    set_label: &crate::t!("inspector.remove"),
                    connect_clicked[sender] => move |_| {
                        let _ = sender.output(InspectorOutput::RemoveCover);
                    },
                },
            },
        }
    }

    fn init(
        state: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self { state };
        let widgets = view_output!();

        let drop_target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let drop_sender = sender.clone();
        drop_target.connect_drop(move |_widget, value, _, _| {
            value
                .get::<gdk::FileList>()
                .ok()
                .and_then(|files| files.files().first().and_then(gio::prelude::FileExt::path))
                .map(|path| {
                    drop_sender
                        .output(InspectorOutput::CoverDropped(path))
                        .is_ok()
                })
                .unwrap_or(false)
        });
        widgets.cover_frame.add_controller(drop_target);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, input: Self::Input, _sender: ComponentSender<Self>) {
        match input {
            InspectorInput::SetState(state) => self.state = state,
        }
    }

    fn post_view() {
        cover_dimensions.set_label(&update_cover(cover, &model.state.cover));
    }
}
