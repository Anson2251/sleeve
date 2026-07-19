use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    path::PathBuf,
    rc::Rc,
};

use relm4::{
    Component, ComponentParts, ComponentSender, RelmWidgetExt, adw,
    factory::FactoryVecDeque,
    gtk::{self, gdk, gio, prelude::*},
};

use crate::{
    models::{AudioFile, BackupVersion, CoverDraft, FileTreeNode, TagDraft, TagField},
    services::{
        create_backup, list_backups, read_audio_file, restore_backup, scan_directory, write_draft,
    },
    ui,
};

pub struct AppModel {
    root_directory: Option<PathBuf>,
    tree: Option<FileTreeNode>,
    expanded_paths: Vec<PathBuf>,
    selected_file: Option<AudioFile>,
    selected_path: Option<PathBuf>,
    sidebar_visible: bool,
    inspector_visible: bool,
    original_draft: Option<TagDraft>,
    active_draft: TagDraft,
    saved_drafts: HashMap<PathBuf, TagDraft>,
    status: String,
    cover_error: Option<String>,
    tree_revision: u64,
    tree_rows: FactoryVecDeque<ui::tree_row::TreeRowComponent>,
    album_cover_textures: Rc<RefCell<HashMap<(PathBuf, i32), gdk::Texture>>>,
    cover_revision: u64,
    backups: Vec<BackupVersion>,
    backup_revision: u64,
    draft_revision: u64,
}

#[derive(Debug)]
pub enum AppMsg {
    ChooseDirectory,
    DirectoryChosen(PathBuf),

    SelectAudioFile(PathBuf),
    SetSidebarVisible(bool),
    SetInspectorVisible(bool),
    SetField(TagField, String),
    Save,
    RestoreDraft,
    ChooseCover,
    CoverChosen(PathBuf),
    RemoveCover,
    RestoreBackup(PathBuf),
    TreeRow(ui::tree_row::TreeRowOutput),
}

#[derive(Debug)]
pub enum CmdMsg {
    DirectoryScanned(Result<Option<FileTreeNode>, String>, PathBuf),
    AudioLoaded(Box<Result<AudioFile, String>>),
    BackupsLoaded(Result<Vec<BackupVersion>, String>),
    SaveFinished(Box<Result<AudioFile, String>>),
    RestoreFinished(Box<Result<AudioFile, String>>),
}

impl AppModel {
    fn selected_path(&self) -> String {
        self.selected_file
            .as_ref()
            .map(|file| file.relative_path.display().to_string())
            .unwrap_or_else(|| "尚未选择文件".into())
    }

    fn header_title(&self) -> String {
        let Some(_) = self.selected_file else {
            return "Sleeve".into();
        };

        let artist = self.active_draft.artist.trim();
        let title = self.active_draft.title.trim();
        match (artist.is_empty(), title.is_empty()) {
            (true, true) => "Sleeve".into(),
            (false, true) => format!("Sleeve · {artist}"),
            (true, false) => format!("Sleeve · {title}"),
            (false, false) => format!("Sleeve · {artist} · {title}"),
        }
    }

    fn metadata(&self, value: impl Fn(&crate::models::AudioMetadata) -> Option<&str>) -> String {
        self.selected_file
            .as_ref()
            .and_then(|file| value(&file.metadata))
            .unwrap_or("—")
            .into()
    }

    fn container(&self) -> String {
        self.selected_file
            .as_ref()
            .map(|file| file.metadata.container.clone())
            .unwrap_or_else(|| "—".into())
    }

    fn encoder(&self) -> String {
        self.selected_file
            .as_ref()
            .map(|file| file.metadata.codec.trim())
            .filter(|codec| !codec.is_empty())
            .unwrap_or("-")
            .into()
    }

    fn cover_hint(&self) -> &str {
        if let Some(error) = &self.cover_error {
            error
        } else {
            match self.active_draft.cover {
                CoverDraft::External(_) => "内存草稿封面：尚未写入音频文件",
                CoverDraft::Embedded(_) => "文件中的嵌入封面（只读预览）",
                CoverDraft::Removed => "封面已从当前内存草稿移除",
                CoverDraft::Unavailable => "将图片拖放到封面上，或选择图片",
            }
        }
    }

    fn clear_selection(&mut self) {
        self.selected_file = None;
        self.inspector_visible = false;
        self.set_selected_path(None);
        self.original_draft = None;
        self.active_draft = TagDraft::default();
        self.cover_error = None;
        self.backups.clear();
        self.backup_revision = self.backup_revision.wrapping_add(1);
        self.cover_revision = self.cover_revision.wrapping_add(1);
        self.draft_revision = self.draft_revision.wrapping_add(1);
    }

    fn load_file(&mut self, file: AudioFile, status: &str) {
        let original = TagDraft::from_audio_file(&file);
        self.active_draft = self
            .saved_drafts
            .get(&file.path)
            .cloned()
            .unwrap_or_else(|| original.clone());
        self.original_draft = Some(original);
        self.saved_drafts.remove(&file.path);
        self.status = if status.contains("{}") {
            status.replacen("{}", &file.relative_path.display().to_string(), 1)
        } else {
            status.into()
        };
        self.set_selected_path(Some(file.path.clone()));
        self.selected_file = Some(file);
        self.cover_error = None;
        self.cover_revision = self.cover_revision.wrapping_add(1);
        self.draft_revision = self.draft_revision.wrapping_add(1);
    }

    fn sync_tree_rows(&mut self) {
        let rows = self
            .tree
            .as_ref()
            .map(|tree| tree.flatten(&self.expanded_paths))
            .unwrap_or_default();
        let selected_path = self.selected_path.as_deref();
        let mut tree_rows = self.tree_rows.guard();
        tree_rows.clear();
        for row in rows {
            let selected = !row.is_directory && selected_path == Some(row.path.as_path());
            tree_rows.push_back(ui::tree_row::TreeRowInit {
                row,
                selected,
                textures: self.album_cover_textures.clone(),
            });
        }
    }

    fn toggle_directory(&mut self, path: &std::path::Path) {
        let Some(tree) = self.tree.as_ref() else {
            return;
        };
        let old_rows = tree.flatten(&self.expanded_paths);
        let Some(index) = old_rows.iter().position(|row| row.path == path) else {
            return;
        };
        let depth = old_rows[index].depth;
        let old_descendant_count = old_rows[index + 1..]
            .iter()
            .take_while(|row| row.depth > depth)
            .count();

        if let Some(expanded_index) = self.expanded_paths.iter().position(|item| item == path) {
            self.expanded_paths.remove(expanded_index);
        } else {
            self.expanded_paths.push(path.to_owned());
        }

        let new_rows = tree.flatten(&self.expanded_paths);
        let new_descendants = new_rows[index + 1..]
            .iter()
            .take_while(|row| row.depth > depth)
            .cloned()
            .collect::<Vec<_>>();
        let selected_path = self.selected_path.as_deref();
        let mut tree_rows = self.tree_rows.guard();
        if let Some(row) = tree_rows.get_mut(index) {
            row.set_expanded(new_rows[index].expanded);
        }
        for _ in 0..old_descendant_count {
            tree_rows.remove(index + 1);
        }
        for (offset, row) in new_descendants.into_iter().enumerate() {
            let selected = !row.is_directory && selected_path == Some(row.path.as_path());
            tree_rows.insert(
                index + 1 + offset,
                ui::tree_row::TreeRowInit {
                    row,
                    selected,
                    textures: self.album_cover_textures.clone(),
                },
            );
        }
    }

    fn set_selected_path(&mut self, path: Option<PathBuf>) {
        let previous = std::mem::replace(&mut self.selected_path, path.clone());
        let changed_rows = self
            .tree_rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let is_selected = path.as_deref() == Some(row.path());
                let was_selected = previous.as_deref() == Some(row.path());
                (is_selected != was_selected).then_some((index, is_selected))
            })
            .collect::<Vec<_>>();
        let mut tree_rows = self.tree_rows.guard();
        for (index, is_selected) in changed_rows {
            if let Some(row) = tree_rows.get_mut(index) {
                row.set_selected(is_selected);
            }
        }
    }

    fn refresh_backups(&self, sender: ComponentSender<Self>) {
        let (Some(root), Some(file)) = (&self.root_directory, &self.selected_file) else {
            return;
        };
        let root = root.clone();
        let source = file.path.clone();
        sender.spawn_oneshot_command(move || CmdMsg::BackupsLoaded(list_backups(&root, &source)));
    }
}

#[relm4::component(pub)]
impl Component for AppModel {
    type Init = ();
    type Input = AppMsg;
    type Output = ();
    type CommandOutput = CmdMsg;

    additional_fields! {
        rendered_cover_revision: Cell<u64>,
        rendered_backup_revision: Cell<u64>,
        rendered_draft_revision: Cell<u64>,
        sidebar_button: gtk::ToggleButton,
        inspector_button: gtk::ToggleButton,
        syncing: Rc<Cell<bool>>,
        status_label: gtk::Label,
        restore_popover: gtk::Popover,
        backup_list: gtk::Box,
        restore_button: gtk::Button,
    }

    view! {
        gtk::Window {
            set_title: Some("Sleeve · Audio Tag Editor"),
            set_default_size: (1280, 760),
            set_resizable: true,
            set_fullscreened: false,


            gtk::Overlay {
                #[name = "content"]
                #[wrap(Some)]
                set_child = &adw::OverlaySplitView {
                    #[watch]
                    set_visible: model.root_directory.is_some(),
                    #[watch]
                    set_show_sidebar: model.sidebar_visible,
                    set_sidebar_width_fraction: 0.24,
                    set_min_sidebar_width: 240.0,
                    set_max_sidebar_width: 360.0,
                    #[wrap(Some)]
                    set_sidebar = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_width_request: 290,
                    set_margin_all: 0,
                    gtk::ScrolledWindow {
                        set_vexpand: true,
                        #[local_ref]
                        tree_box -> gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 2,
                        },
                    },
                },
                #[wrap(Some)]
                set_content = &adw::OverlaySplitView {
                    set_sidebar_position: gtk::PackType::End,
                    #[watch]
                    set_show_sidebar: model.inspector_visible && model.selected_file.is_some(),
                    set_sidebar_width_fraction: 0.28,
                    set_min_sidebar_width: 280.0,
                    set_max_sidebar_width: 420.0,
                    #[wrap(Some)]
                    set_content = &gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,

                        #[name = "editor"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 8,
                            set_hexpand: true,
                            set_width_request: 480,
                            set_margin_all: 20,
                            #[watch]
                            set_visible: model.selected_file.is_some(),
                            gtk::Label {
                                #[watch]
                                set_label: &model.selected_path(),
                                set_halign: gtk::Align::Start,
                                add_css_class: "title-4",
                            },
                            gtk::Label { set_label: " " },
                            gtk::Label { set_label: "标题", set_halign: gtk::Align::Start },
                            #[name = "title_entry"]
                            gtk::Entry {
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::Title, entry.text().to_string())); }
                                },
                            },
                            gtk::Label {
                                #[watch]
                                set_label: model.active_draft.validation_error(TagField::Title).unwrap_or(""),
                                #[watch]
                                set_visible: model.active_draft.validation_error(TagField::Title).is_some(),
                                add_css_class: "error",
                                set_halign: gtk::Align::Start,
                            },
                            gtk::Label { set_label: "艺人", set_halign: gtk::Align::Start },
                            #[name = "artist_entry"]
                            gtk::Entry {
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::Artist, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::Artist).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::Artist).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "专辑", set_halign: gtk::Align::Start },
                            #[name = "album_entry"]
                            gtk::Entry {
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::Album, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::Album).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::Album).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "专辑艺人", set_halign: gtk::Align::Start },
                            #[name = "album_artist_entry"]
                            gtk::Entry {
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::AlbumArtist, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::AlbumArtist).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::AlbumArtist).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "年份", set_halign: gtk::Align::Start },
                            #[name = "year_entry"]
                            gtk::Entry {
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::Year, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::Year).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::Year).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "曲目号", set_halign: gtk::Align::Start },
                            #[name = "track_entry"]
                            gtk::Entry {
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::TrackNumber, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::TrackNumber).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::TrackNumber).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "碟号", set_halign: gtk::Align::Start },
                            #[name = "disc_entry"]
                            gtk::Entry {
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::DiscNumber, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::DiscNumber).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::DiscNumber).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "流派", set_halign: gtk::Align::Start },
                            #[name = "genre_entry"]
                            gtk::Entry {
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::Genre, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::Genre).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::Genre).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Box {
                                set_spacing: 8,
                                set_halign: gtk::Align::End,
                                gtk::Button { set_label: "还原", connect_clicked => AppMsg::RestoreDraft },
                                gtk::Button { set_label: "保存", connect_clicked => AppMsg::Save },
                            },
                        },
                        gtk::Label {
                            set_label: "请从左侧选择一个音频文件",
                            set_hexpand: true,
                            set_vexpand: true,
                            set_halign: gtk::Align::Center,
                            set_valign: gtk::Align::Center,
                            add_css_class: "title-4",
                            #[watch]
                            set_visible: model.selected_file.is_none(),
                        },
                    },
                    #[wrap(Some)]
                    set_sidebar = &gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 12,
                        set_width_request: 310,
                        set_margin_all: 16,
                        #[watch]
                        set_sensitive: model.selected_file.is_some(),
                        gtk::Label { set_label: "元信息与封面", set_halign: gtk::Align::Start, add_css_class: "title-4" },
                        gtk::Box {
                            set_spacing: 8,
                            gtk::Label { set_label: "容器格式", set_hexpand: true, set_halign: gtk::Align::Start },
                            gtk::Label { #[watch] set_label: &model.container(), set_halign: gtk::Align::End },
                        },
                        gtk::Box {
                            set_spacing: 8,
                            gtk::Label { set_label: "编码器", set_hexpand: true, set_halign: gtk::Align::Start },
                            gtk::Label { #[watch] set_label: &model.encoder(), set_halign: gtk::Align::End },
                        },
                        gtk::Box {
                            set_spacing: 8,
                            gtk::Label { set_label: "时长", set_hexpand: true, set_halign: gtk::Align::Start },
                            gtk::Label { #[watch] set_label: &model.metadata(|metadata| metadata.duration.as_deref()), set_halign: gtk::Align::End },
                        },
                        gtk::Box {
                            set_spacing: 8,
                            gtk::Label { set_label: "平均码率", set_hexpand: true, set_halign: gtk::Align::Start },
                            gtk::Label { #[watch] set_label: &model.metadata(|metadata| metadata.bitrate.as_deref()), set_halign: gtk::Align::End },
                        },
                        gtk::Box {
                            set_spacing: 8,
                            gtk::Label { set_label: "采样率", set_hexpand: true, set_halign: gtk::Align::Start },
                            gtk::Label { #[watch] set_label: &model.metadata(|metadata| metadata.sample_rate.as_deref()), set_halign: gtk::Align::End },
                        },
                        gtk::Box {
                            set_spacing: 8,
                            gtk::Label { set_label: "声道", set_hexpand: true, set_halign: gtk::Align::Start },
                            gtk::Label { #[watch] set_label: &model.metadata(|metadata| metadata.channels.as_deref()), set_halign: gtk::Align::End },
                        },
                        gtk::Box {
                            set_spacing: 8,
                            gtk::Label { set_label: "位深", set_hexpand: true, set_halign: gtk::Align::Start },
                            gtk::Label { #[watch] set_label: &model.metadata(|metadata| metadata.bits_per_sample.as_deref()), set_halign: gtk::Align::End },
                        },
                        gtk::Box {
                            set_spacing: 8,
                            gtk::Label { set_label: "文件大小", set_hexpand: true, set_halign: gtk::Align::Start },
                            gtk::Label { #[watch] set_label: &model.metadata(|metadata| metadata.file_size.as_deref()), set_halign: gtk::Align::End },
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
                            set_label: model.cover_hint(),
                            set_wrap: true,
                            set_justify: gtk::Justification::Center,
                        },
                        gtk::Box {
                            set_spacing: 8,
                            set_halign: gtk::Align::Center,
                            gtk::Button { set_label: "选择图片", connect_clicked => AppMsg::ChooseCover },
                            gtk::Button { set_label: "移除", connect_clicked => AppMsg::RemoveCover },
                        },
                        },
                    },
                },
                add_overlay = &gtk::Label {
                    set_label: "请先从顶部工具栏打开一个音乐文件夹",
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    add_css_class: "title-4",
                    #[watch]
                    set_visible: model.root_directory.is_none(),
                },

            },
        }
    }

    fn init(
        _: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let album_cover_textures = Rc::new(RefCell::new(HashMap::new()));
        let model = Self {
            root_directory: None,
            tree: None,
            expanded_paths: Vec::new(),
            selected_file: None,
            selected_path: None,
            sidebar_visible: false,
            inspector_visible: false,
            original_draft: None,
            active_draft: TagDraft::default(),
            saved_drafts: HashMap::new(),
            status: "选择一个音乐目录以开始浏览。".into(),
            cover_error: None,
            tree_revision: 0,
            tree_rows: FactoryVecDeque::builder()
                .launch(
                    gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(2)
                        .build(),
                )
                .forward(sender.input_sender(), AppMsg::TreeRow),
            album_cover_textures: album_cover_textures.clone(),
            cover_revision: 0,
            backups: Vec::new(),
            backup_revision: 0,
            draft_revision: 0,
        };
        let syncing = Rc::new(Cell::new(false));
        let rendered_cover_revision = Cell::new(u64::MAX);
        let rendered_backup_revision = Cell::new(u64::MAX);
        let rendered_draft_revision = Cell::new(u64::MAX);

        if let Some(display) = gdk::Display::default() {
            gtk::IconTheme::for_display(&display)
                .add_resource_path("/com/github/anson2251/sleeve/icons");
        }

        let header_bar = gtk::HeaderBar::new();
        header_bar.set_show_title_buttons(true);
        #[cfg(target_os = "macos")]
        header_bar.set_property("use-native-controls", true);

        let sidebar_button = gtk::ToggleButton::builder()
            .icon_name("sidebar-show-symbolic")
            .tooltip_text("显示或隐藏文件列表")
            .build();
        let sidebar_sender = sender.clone();
        sidebar_button.connect_toggled(move |button| {
            sidebar_sender.input(AppMsg::SetSidebarVisible(button.is_active()))
        });
        header_bar.pack_start(&sidebar_button);

        let open_directory = gtk::Button::builder()
            .icon_name("folder-open-symbolic")
            .tooltip_text("打开目录")
            .build();
        let open_sender = sender.clone();
        open_directory.connect_clicked(move |_| open_sender.input(AppMsg::ChooseDirectory));
        header_bar.pack_start(&open_directory);

        let inspector_button = gtk::ToggleButton::builder()
            .icon_name("dialog-information-symbolic")
            .tooltip_text("显示或隐藏元信息与封面")
            .build();
        let inspector_sender = sender.clone();
        inspector_button.connect_toggled(move |button| {
            inspector_sender.input(AppMsg::SetInspectorVisible(button.is_active()))
        });
        header_bar.pack_end(&inspector_button);

        let style_provider = gtk::CssProvider::new();
        style_provider.load_from_data(
            ".file-tree-row { min-height: 34px; padding: 0 8px; border: none; border-radius: 6px; box-shadow: none; background: transparent; }\
             .file-tree-row:hover { background: alpha(@theme_fg_color, 0.06); }\
             .file-tree-row.selected { background: alpha(@accent_bg_color, 0.22); color: @accent_fg_color; }\
             .file-tree-row:focus { box-shadow: none; outline: none; }\
             .regular-file { font-weight: normal; }\
             .tree-thumbnail, .album-thumbnail { min-width: 24px; min-height: 24px; max-width: 24px; max-height: 24px; border-radius: 4px;}",
        );
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &style_provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let restore_button = gtk::Button::builder()
            .icon_name("document-revert-symbolic")
            .tooltip_text("恢复备份")
            .build();
        restore_button.set_sensitive(false);
        let restore_popover = gtk::Popover::new();
        let backup_list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)
            .margin_top(8)
            .margin_bottom(8)
            .margin_start(8)
            .margin_end(8)
            .build();
        restore_popover.set_child(Some(&backup_list));
        restore_popover.set_parent(&restore_button);
        let popover_for_click = restore_popover.clone();
        restore_button.connect_clicked(move |_| popover_for_click.popup());
        header_bar.pack_end(&restore_button);

        let status_label = gtk::Label::builder()
            .label(model.header_title())
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        header_bar.set_title_widget(Some(&status_label));
        root.set_titlebar(Some(&header_bar));

        let tree_box = model.tree_rows.widget();
        let widgets = view_output!();

        configure_macos_window(&root);
        configure_macos_window_style();

        let drop_target = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let drop_sender = sender.clone();
        drop_target.connect_drop(move |_widget, value, _, _| {
            value
                .get::<gdk::FileList>()
                .ok()
                .and_then(|files| files.files().first().and_then(gio::File::path))
                .map(|path| drop_sender.input(AppMsg::CoverChosen(path)))
                .is_some()
        });
        widgets.cover_frame.add_controller(drop_target);
        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: AppMsg, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            AppMsg::ChooseDirectory => choose_directory(root, sender),
            AppMsg::DirectoryChosen(path) => {
                self.sidebar_visible = true;
                self.status = format!("正在扫描 {}…", path.display());
                self.root_directory = Some(path.clone());
                self.tree = None;
                self.expanded_paths.clear();
                self.tree_revision = self.tree_revision.wrapping_add(1);
                self.clear_selection();
                sender.spawn_oneshot_command(move || {
                    CmdMsg::DirectoryScanned(scan_directory(path.clone()), path)
                });
            }

            AppMsg::SelectAudioFile(path) => {
                self.set_selected_path(Some(path.clone()));
                let Some(root_path) = self.root_directory.clone() else {
                    return;
                };
                self.status = format!("正在读取 {}…", path.display());
                sender.spawn_oneshot_command(move || {
                    CmdMsg::AudioLoaded(Box::new(read_audio_file(path, root_path)))
                });
            }
            AppMsg::SetSidebarVisible(visible) => self.sidebar_visible = visible,
            AppMsg::SetInspectorVisible(visible) => {
                self.inspector_visible = visible && self.selected_file.is_some();
            }
            AppMsg::TreeRow(output) => match output {
                ui::tree_row::TreeRowOutput::ToggleDirectory(path) => self.toggle_directory(&path),
                ui::tree_row::TreeRowOutput::SelectAudioFile(path) => {
                    sender.input(AppMsg::SelectAudioFile(path))
                }
            },
            AppMsg::SetField(field, value) => self.active_draft.set(field, value),
            AppMsg::Save => {
                if !self.active_draft.is_valid() {
                    self.status = "请先修正表单中的无效字段。".into();
                } else if let (Some(file), Some(root)) = (&self.selected_file, &self.root_directory)
                {
                    let source = file.path.clone();
                    let root = root.clone();
                    let draft = self.active_draft.clone();
                    self.status = "正在备份并保存标签…".into();
                    sender.spawn_oneshot_command(move || {
                        let result = create_backup(&root, &source)
                            .and_then(|_| write_draft(&source, &draft))
                            .and_then(|_| read_audio_file(source, root));
                        CmdMsg::SaveFinished(Box::new(result))
                    });
                }
            }
            AppMsg::RestoreDraft => {
                if let (Some(file), Some(original)) = (&self.selected_file, &self.original_draft) {
                    self.active_draft = original.clone();
                    self.saved_drafts.remove(&file.path);
                    self.cover_error = None;
                    self.cover_revision = self.cover_revision.wrapping_add(1);
                    self.draft_revision = self.draft_revision.wrapping_add(1);
                    self.status = "已恢复为文件中的原始标签。".into();
                }
            }
            AppMsg::ChooseCover => choose_cover(root, sender),
            AppMsg::CoverChosen(path) => {
                if image::ImageReader::open(&path)
                    .ok()
                    .and_then(|reader| reader.with_guessed_format().ok())
                    .and_then(|reader| reader.decode().ok())
                    .is_some()
                {
                    self.active_draft.cover = CoverDraft::External(path);
                    self.cover_error = None;
                    self.cover_revision = self.cover_revision.wrapping_add(1);
                } else {
                    self.cover_error = Some("请选择有效的图片文件。".into());
                }
            }
            AppMsg::RemoveCover => {
                self.active_draft.cover = CoverDraft::Removed;
                self.cover_revision = self.cover_revision.wrapping_add(1);
            }
            AppMsg::RestoreBackup(backup) => {
                if let (Some(file), Some(root)) = (&self.selected_file, &self.root_directory) {
                    let source = file.path.clone();
                    let root = root.clone();
                    self.status = "正在备份当前文件并恢复版本…".into();
                    sender.spawn_oneshot_command(move || {
                        let result = restore_backup(&root, &source, &backup)
                            .and_then(|_| read_audio_file(source, root));
                        CmdMsg::RestoreFinished(Box::new(result))
                    });
                }
            }
        }
    }

    fn update_cmd(&mut self, msg: CmdMsg, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            CmdMsg::DirectoryScanned(result, path) => match result {
                Ok(Some(tree)) => {
                    self.expanded_paths = tree.album_directory_paths();
                    if !self.expanded_paths.iter().any(|expanded| expanded == &path) {
                        self.expanded_paths.push(path);
                    }
                    self.tree = Some(tree);
                    self.sync_tree_rows();
                    self.status = "目录扫描完成。选择一个音频文件以编辑其内存草稿。".into();
                }
                Ok(None) => self.status = "该目录及其子目录中没有受支持的音频文件。".into(),
                Err(error) => self.status = error,
            },
            CmdMsg::AudioLoaded(result) => match *result {
                Ok(file) => {
                    self.load_file(file, "正在编辑 {}。");
                    self.refresh_backups(sender.clone());
                }
                Err(error) => {
                    self.clear_selection();
                    self.status = error;
                }
            },
            CmdMsg::BackupsLoaded(result) => match result {
                Ok(backups) => {
                    self.backups = backups;
                    self.backup_revision = self.backup_revision.wrapping_add(1);
                }
                Err(error) => self.status = error,
            },
            CmdMsg::SaveFinished(result) => match *result {
                Ok(file) => {
                    self.load_file(file, "已保存标签与封面。");
                    self.refresh_backups(sender.clone());
                }
                Err(error) => self.status = error,
            },
            CmdMsg::RestoreFinished(result) => match *result {
                Ok(file) => {
                    self.load_file(file, "已恢复备份版本。");
                    self.refresh_backups(sender.clone());
                }
                Err(error) => self.status = error,
            },
        }
    }

    fn post_view() {
        status_label.set_label(&model.header_title());
        sidebar_button.set_sensitive(model.root_directory.is_some());
        sidebar_button.set_active(model.sidebar_visible);
        inspector_button.set_sensitive(model.selected_file.is_some());
        inspector_button.set_active(model.inspector_visible);

        if rendered_cover_revision.replace(model.cover_revision) != model.cover_revision {
            cover_dimensions.set_label(&update_cover(cover, &model.active_draft.cover));
        }
        if rendered_backup_revision.replace(model.backup_revision) != model.backup_revision {
            restore_button.set_sensitive(!model.backups.is_empty());
            refresh_backup_list(backup_list, &model.backups, sender.clone());
        }
        if rendered_draft_revision.replace(model.draft_revision) != model.draft_revision {
            syncing.set(true);
            sync_entry(title_entry, &model.active_draft.title);
            sync_entry(artist_entry, &model.active_draft.artist);
            sync_entry(album_entry, &model.active_draft.album);
            sync_entry(album_artist_entry, &model.active_draft.album_artist);
            sync_entry(year_entry, &model.active_draft.year);
            sync_entry(track_entry, &model.active_draft.track_number);
            sync_entry(disc_entry, &model.active_draft.disc_number);
            sync_entry(genre_entry, &model.active_draft.genre);
            syncing.set(false);
        }
    }
}

#[cfg(target_os = "macos")]
fn configure_macos_window(window: &gtk::Window) {
    use objc2_app_kit::{NSWindow, NSWindowCollectionBehavior};

    window.connect_realize(|window| {
        let Some(surface) = window.surface() else {
            return;
        };
        let Some(macos_surface) = surface.downcast_ref::<gdk4_macos::MacosSurface>() else {
            return;
        };
        let native_window = macos_surface.native();
        let ns_window = unsafe { &*(native_window as *const NSWindow) };
        ns_window.setCollectionBehavior(NSWindowCollectionBehavior::FullScreenNone);
    });
}

#[cfg(target_os = "macos")]
fn configure_macos_window_style() {
    let provider = gtk::CssProvider::new();
    provider.load_from_data(
        "window, .background, .titlebar, headerbar, .window-frame { border-radius: 0px; }",
    );
    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn choose_directory(root: &gtk::Window, sender: ComponentSender<AppModel>) {
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

fn choose_cover(root: &gtk::Window, sender: ComponentSender<AppModel>) {
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

fn sync_entry(entry: &gtk::Entry, value: &str) {
    if entry.text().as_str() != value {
        entry.set_text(value);
    }
}

fn refresh_backup_list(
    container: &gtk::Box,
    backups: &[BackupVersion],
    sender: ComponentSender<AppModel>,
) {
    ui::file_tree::clear(container);
    for backup in backups {
        let button = gtk::Button::with_label(&format!(
            "恢复 {} · {}",
            backup.timestamp,
            format_byte_size(backup.size_bytes)
        ));
        let backup_path = backup.path.clone();
        let restore_sender = sender.clone();
        button.connect_clicked(move |_| {
            restore_sender.input(AppMsg::RestoreBackup(backup_path.clone()))
        });
        container.append(&button);
    }
}

const COVER_PREVIEW_MAX_SIZE: i32 = 520;

fn update_cover(picture: &gtk::Picture, cover: &CoverDraft) -> String {
    picture.set_filename(None::<&str>);
    picture.set_pixbuf(None::<&gdk_pixbuf::Pixbuf>);

    let (pixbuf, byte_size) = match cover {
        CoverDraft::External(path) => (
            gdk_pixbuf::Pixbuf::from_file(path).ok(),
            std::fs::metadata(path).ok().map(|metadata| metadata.len()),
        ),
        CoverDraft::Embedded(bytes) => (
            gdk_pixbuf::Pixbuf::from_read(std::io::Cursor::new(bytes.clone())).ok(),
            Some(bytes.len() as u64),
        ),
        CoverDraft::Unavailable | CoverDraft::Removed => (None, None),
    };

    match pixbuf {
        Some(pixbuf) => {
            let dimensions = format!("{} × {} px", pixbuf.width(), pixbuf.height());
            let size = byte_size
                .map(format_byte_size)
                .unwrap_or_else(|| "大小未知".into());
            picture.set_pixbuf(Some(&scale_cover_preview(&pixbuf)));
            format!("{dimensions} · {size}")
        }
        None => "无封面图像".into(),
    }
}

fn format_byte_size(bytes: u64) -> String {
    const MIB: u64 = 1024 * 1024;
    const KIB: u64 = 1024;
    if bytes >= MIB {
        format!("{:.1} MB", bytes as f64 / MIB as f64)
    } else if bytes >= KIB {
        format!("{:.1} KB", bytes as f64 / KIB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn scale_cover_preview(pixbuf: &gdk_pixbuf::Pixbuf) -> gdk_pixbuf::Pixbuf {
    let width = pixbuf.width();
    let height = pixbuf.height();
    let scale = (COVER_PREVIEW_MAX_SIZE as f64 / width as f64)
        .min(COVER_PREVIEW_MAX_SIZE as f64 / height as f64)
        .min(1.0);

    if scale == 1.0 {
        return pixbuf.clone();
    }

    let scaled_width = (width as f64 * scale).round() as i32;
    let scaled_height = (height as f64 * scale).round() as i32;
    pixbuf
        .scale_simple(
            scaled_width.max(1),
            scaled_height.max(1),
            gdk_pixbuf::InterpType::Bilinear,
        )
        .unwrap_or_else(|| pixbuf.clone())
}
