use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc};

use relm4::{
    RelmWidgetExt,
    factory::{DynamicIndex, FactoryComponent, FactorySender},
    gtk::{self, gdk, prelude::*},
};

use crate::models::TreeRow;

const NAVIGATION_ICON_SIZE: i32 = 16;
pub const ALBUM_ICON_SIZE: i32 = 20;

#[derive(Debug)]
pub enum TreeRowOutput {
    ToggleDirectory(PathBuf),
    SelectAudioFile(PathBuf),
}

#[derive(Debug)]
pub enum TreeRowMsg {
    Activate,
}

pub struct TreeRowInit {
    pub row: TreeRow,
    pub selected: bool,
    pub textures: Rc<RefCell<HashMap<(PathBuf, i32), gdk::Texture>>>,
}

pub struct TreeRowComponent {
    row: TreeRow,
    selected: bool,
    texture: Option<gdk::Texture>,
}

impl TreeRowComponent {
    fn standard_icon_name(&self) -> &'static str {
        if self.row.is_directory {
            "folder-symbolic"
        } else {
            "audio-x-generic-symbolic"
        }
    }

    pub fn path(&self) -> &std::path::Path {
        &self.row.path
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    pub fn set_expanded(&mut self, expanded: bool) {
        self.row.expanded = expanded;
    }
}

#[relm4::factory(pub)]
impl FactoryComponent for TreeRowComponent {
    type Init = TreeRowInit;
    type Input = TreeRowMsg;
    type Output = TreeRowOutput;
    type CommandOutput = ();
    type ParentWidget = gtk::Box;

    view! {
        #[root]
        gtk::Button {
            set_halign: gtk::Align::Fill,
            add_css_class: "flat",
            add_css_class: "file-tree-row",
            #[watch]
            set_class_active: ("selected", self.selected),
            connect_clicked => TreeRowMsg::Activate,
            gtk::Box {
                set_spacing: 6,
                #[watch]
                set_margin_start: (self.row.depth * 16) as i32,
                gtk::Image {
                    #[watch]
                    set_visible: self.row.is_directory,
                    #[watch]
                    set_icon_name: Some(if self.row.expanded {
                        "pan-down-symbolic"
                    } else {
                        "pan-end-symbolic"
                    }),
                    set_pixel_size: NAVIGATION_ICON_SIZE,
                },
                relm4::adw::Clamp {
                    add_css_class: "tree-thumbnail",
                    set_maximum_size: ALBUM_ICON_SIZE,
                    set_tightening_threshold: ALBUM_ICON_SIZE,
                    set_hexpand: false,
                    set_vexpand: false,
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Center,
                    #[watch]
                    set_visible: self.texture.is_some(),
                    #[wrap(Some)]
                    set_child = &gtk::Picture {
                        set_size_request: (ALBUM_ICON_SIZE, ALBUM_ICON_SIZE),
                        set_keep_aspect_ratio: true,
                        set_can_shrink: true,
                        #[watch]
                        set_paintable: self.texture.as_ref(),
                    },
                },
                gtk::Image {
                    add_css_class: "album-thumbnail",
                    set_size_request: (ALBUM_ICON_SIZE, ALBUM_ICON_SIZE),
                    set_hexpand: false,
                    set_vexpand: false,
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Center,
                    #[watch]
                    set_visible: self.row.is_album && self.texture.is_none(),
                    set_icon_name: Some("album-outlined-symbolic"),
                    set_pixel_size: ALBUM_ICON_SIZE,
                },
                gtk::Image {
                    set_size_request: (NAVIGATION_ICON_SIZE, NAVIGATION_ICON_SIZE),
                    set_hexpand: false,
                    set_vexpand: false,
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Center,
                    #[watch]
                    set_visible: !self.row.is_album,
                    #[watch]
                    set_icon_name: Some(self.standard_icon_name()),
                    set_pixel_size: NAVIGATION_ICON_SIZE,
                },
                gtk::Label {
                    #[watch]
                    set_class_active: ("regular-file", !self.row.is_directory && !self.row.is_album),
                    #[watch]
                    set_label: &self.row.name,
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                }
            }
        }
    }

    fn init_model(init: Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        let texture = init.row.album_cover.as_deref().and_then(|cover| {
            let key = (init.row.path.clone(), 0);
            if let Some(texture) = init.textures.borrow().get(&key).cloned() {
                return Some(texture);
            }
            let texture = gdk::Texture::from_bytes(&glib::Bytes::from(cover)).ok()?;
            init.textures.borrow_mut().insert(key, texture.clone());
            Some(texture)
        });
        Self {
            row: init.row,
            selected: init.selected,
            texture,
        }
    }

    fn update(&mut self, msg: Self::Input, sender: FactorySender<Self>) {
        match msg {
            TreeRowMsg::Activate => {
                let output = if self.row.is_directory {
                    TreeRowOutput::ToggleDirectory(self.row.path.clone())
                } else {
                    TreeRowOutput::SelectAudioFile(self.row.path.clone())
                };
                sender.output(output).expect("tree row parent was dropped");
            }
        }
    }
}
