use std::{
    cell::{Cell, RefCell},
    collections::{BTreeSet, HashMap},
    path::PathBuf,
    rc::Rc,
    time::Duration,
};

use relm4::adw::prelude::*;
use relm4::{
    Component, ComponentParts, ComponentSender, RelmWidgetExt, adw,
    factory::FactoryVecDeque,
    gtk::{self, gdk, gio},
};

use crate::{
    models::{
        AudioFile, BackupVersion, CoverDraft, FileTreeNode, TagDraft, TagField,
        audio_paths_between, common_draft,
    },
    services::{clear_backups, create_backup, read_audio_file, scan_directory, write_draft},
    ui,
};

mod cover;
mod dialogs;
mod history;

use cover::{
    BlurredCoverCache, EditorCoverTransition, draw_cover, transition_progress, update_cover,
    update_cover_background,
};
use dialogs::{choose_cover, choose_directory, sync_entry};
use history::{
    BatchHistory, HistoryBatch, is_current_batch_draft_result, restore_history_batch,
    rollback_history_batch,
};

pub struct AppModel {
    root_directory: Option<PathBuf>,
    tree: Option<FileTreeNode>,
    expanded_paths: Vec<PathBuf>,
    selected_file: Option<AudioFile>,
    selected_path: Option<PathBuf>,
    selected_paths: BTreeSet<PathBuf>,
    mixed_fields: std::collections::HashSet<TagField>,
    covers_mixed: bool,
    selection_anchor: Option<PathBuf>,
    sidebar_visible: bool,
    inspector_visible: bool,
    original_draft: Option<TagDraft>,
    active_draft: TagDraft,
    saved_drafts: HashMap<PathBuf, TagDraft>,
    status: String,
    cover_error: Option<String>,
    tree_revision: u64,
    selection_revision: u64,
    batch_draft_revision: u64,
    tree_rows: FactoryVecDeque<ui::tree_row::TreeRowComponent>,
    album_cover_textures: Rc<RefCell<HashMap<(PathBuf, i32), gdk::Texture>>>,
    blurred_cover_cache: RefCell<BlurredCoverCache>,
    cover_revision: u64,
    history: BatchHistory,
    pending_save: Option<PendingSave>,
    pending_batch_save: Option<PendingBatchSave>,
    save_in_progress: bool,
    batch_save_in_progress: bool,
    pending_action: Option<PendingAction>,
    quitting: bool,
    close_dialog_open: bool,
    draft_revision: u64,
}

#[derive(Debug)]
pub(crate) struct PendingSave {
    source: Option<glib::SourceId>,
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub(crate) struct PendingBatchSave {
    paths: Vec<PathBuf>,
    fields: HashMap<TagField, String>,
    cover: Option<CoverDraft>,
}

impl PendingBatchSave {
    fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.cover.is_none()
    }
}

#[derive(Debug)]
pub enum CloseAction {
    Cancel,
    Discard,
    Save,
}

enum PendingAction {
    Select {
        path: PathBuf,
        modifiers: gdk::ModifierType,
    },
    OpenDirectory(PathBuf),
    Undo,
    Redo,
}

#[derive(Debug)]
pub enum AppMsg {
    ChooseDirectory,
    DirectoryChosen(PathBuf),

    SelectAudioFile {
        path: PathBuf,
        modifiers: gdk::ModifierType,
    },
    SetSidebarVisible(bool),
    SetInspectorVisible(bool),
    ToggleSidebar,
    ToggleInspector,
    SetField(TagField, String),
    SaveNow,
    Undo,
    Redo,
    RequestClose,
    CloseAction(CloseAction),
    ShowAbout,
    ChooseCover,
    CoverChosen(PathBuf),
    RemoveCover,
    TreeRow(ui::tree_row::TreeRowOutput),
}

#[derive(Debug)]
pub enum CmdMsg {
    DirectoryScanned {
        result: Result<Option<FileTreeNode>, String>,
        path: PathBuf,
        revision: u64,
    },
    AudioLoaded {
        result: Box<Result<AudioFile, String>>,
        path: PathBuf,
        revision: u64,
    },
    SaveFinished {
        result: Box<Result<AudioFile, String>>,
        snapshot: Option<BackupVersion>,
        draft: TagDraft,
        pending: PendingSave,
    },
    BatchSaveFinished {
        result: Box<Result<Vec<AudioFile>, String>>,
        batch: HistoryBatch,
        pending: PendingBatchSave,
    },
    BatchDraftsLoaded {
        paths: BTreeSet<PathBuf>,
        drafts: Vec<TagDraft>,
        selection_revision: u64,
        batch_draft_revision: u64,
    },
    HistoryBatchRestored {
        batch: HistoryBatch,
        result: Box<Result<(Vec<AudioFile>, HistoryBatch), String>>,
        is_undo: bool,
    },
    BackupsCleared(Result<(), String>),
}

impl AppModel {
    fn has_pending_save(&self) -> bool {
        self.pending_save.is_some()
            || self
                .pending_batch_save
                .as_ref()
                .is_some_and(|pending| !pending.is_empty())
    }

    fn is_saving(&self) -> bool {
        self.save_in_progress || self.batch_save_in_progress
    }

    fn is_batch_editing(&self) -> bool {
        self.selected_paths.len() > 1
    }

    fn selection_summary(&self) -> String {
        match self.selected_paths.len() {
            0 => "尚未选择文件".into(),
            1 => self.selected_path(),
            count => format!("已选择 {count} 个文件"),
        }
    }

    fn field_placeholder(&self, field: TagField) -> &str {
        self.mixed_fields
            .contains(&field)
            .then_some("多个值")
            .unwrap_or("")
    }

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
        if self.is_batch_editing() && self.covers_mixed {
            "多个封面：选择图片可批量替换，或移除全部封面"
        } else if let Some(error) = &self.cover_error {
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
        self.set_selected_paths(BTreeSet::new());
        self.selection_anchor = None;
        self.mixed_fields.clear();
        self.covers_mixed = false;
        self.original_draft = None;
        self.active_draft = TagDraft::default();
        self.cover_error = None;
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
        let mut tree_rows = self.tree_rows.guard();
        tree_rows.clear();
        for row in rows {
            let selected = !row.is_directory && self.selected_paths.contains(&row.path);
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
        let mut tree_rows = self.tree_rows.guard();
        if let Some(row) = tree_rows.get_mut(index) {
            row.set_expanded(new_rows[index].expanded);
        }
        for _ in 0..old_descendant_count {
            tree_rows.remove(index + 1);
        }
        for (offset, row) in new_descendants.into_iter().enumerate() {
            let selected = !row.is_directory && self.selected_paths.contains(&row.path);
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

    fn select_audio_file(&mut self, path: PathBuf, modifiers: gdk::ModifierType) -> bool {
        let range = modifiers.contains(gdk::ModifierType::SHIFT_MASK);
        let toggle = modifiers.contains(gdk::ModifierType::CONTROL_MASK)
            || modifiers.contains(gdk::ModifierType::META_MASK);
        let mut selected_paths = if range && toggle {
            self.selected_paths.clone()
        } else if range {
            BTreeSet::new()
        } else if toggle {
            self.selected_paths.clone()
        } else {
            BTreeSet::new()
        };

        if range {
            let range_start = self.selection_anchor.as_deref().unwrap_or(path.as_path());
            let paths = self
                .tree
                .as_ref()
                .map(|tree| {
                    audio_paths_between(&tree.flatten(&self.expanded_paths), range_start, &path)
                })
                .unwrap_or_default();
            selected_paths.extend(paths);
        } else if toggle && !selected_paths.insert(path.clone()) {
            selected_paths.remove(&path);
        } else {
            selected_paths.insert(path.clone());
        }

        let remains_selected = selected_paths.contains(&path);
        self.set_selected_paths(selected_paths);
        self.selection_anchor = Some(path.clone());
        self.batch_draft_revision = self.batch_draft_revision.wrapping_add(1);
        self.mixed_fields.clear();
        self.covers_mixed = false;
        if remains_selected {
            self.set_selected_path(Some(path));
            true
        } else if self.selected_path.as_deref() == Some(path.as_path()) {
            let next_focus = self
                .tree
                .as_ref()
                .map(|tree| tree.flatten(&self.expanded_paths))
                .and_then(|rows| {
                    rows.into_iter()
                        .find(|row| !row.is_directory && self.selected_paths.contains(&row.path))
                        .map(|row| row.path)
                });
            if let Some(next_focus) = next_focus {
                self.set_selected_path(Some(next_focus));
                true
            } else {
                self.selected_file = None;
                self.inspector_visible = false;
                self.selected_path = None;
                self.original_draft = None;
                self.active_draft = TagDraft::default();
                self.cover_error = None;
                self.cover_revision = self.cover_revision.wrapping_add(1);
                self.draft_revision = self.draft_revision.wrapping_add(1);
                false
            }
        } else {
            false
        }
    }

    fn load_batch_drafts(&self, sender: ComponentSender<Self>) {
        if self.selected_paths.len() < 2 {
            return;
        }
        let paths = self.selected_paths.clone();
        let root = self.root_directory.clone().unwrap_or_default();
        let selection_revision = self.selection_revision;
        let batch_draft_revision = self.batch_draft_revision;
        sender.spawn_oneshot_command(move || {
            let drafts = paths
                .iter()
                .filter_map(|path| read_audio_file(path.clone(), root.clone()).ok())
                .map(|file| TagDraft::from_audio_file(&file))
                .collect();
            CmdMsg::BatchDraftsLoaded {
                paths,
                drafts,
                selection_revision,
                batch_draft_revision,
            }
        });
    }

    fn set_selected_paths(&mut self, selected_paths: BTreeSet<PathBuf>) {
        let previous = std::mem::replace(&mut self.selected_paths, selected_paths);
        let changed_rows = self
            .tree_rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let is_selected = self.selected_paths.contains(row.path());
                let was_selected = previous.contains(row.path());
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

    fn set_selected_path(&mut self, path: Option<PathBuf>) {
        self.selected_path = path;
    }

    fn schedule_save(&mut self, sender: ComponentSender<Self>) {
        let Some(file) = self.selected_file.as_ref() else {
            return;
        };
        if let Some(pending) = self.pending_save.take() {
            if let Some(source) = pending.source {
                source.remove();
            }
        }
        self.pending_batch_save = None;
        let path = file.path.clone();
        let save_sender = sender.clone();
        let source = glib::timeout_add_local_once(Duration::from_millis(500), move || {
            save_sender.input(AppMsg::SaveNow);
        });
        self.pending_save = Some(PendingSave {
            source: Some(source),
            path,
        });
    }

    fn stage_batch_field(&mut self, field: TagField, value: String) {
        let pending = self
            .pending_batch_save
            .get_or_insert_with(|| PendingBatchSave {
                paths: self.selected_paths.iter().cloned().collect(),
                fields: HashMap::new(),
                cover: None,
            });
        pending.fields.insert(field, value);
    }

    fn stage_batch_cover(&mut self, cover: CoverDraft) {
        let pending = self
            .pending_batch_save
            .get_or_insert_with(|| PendingBatchSave {
                paths: self.selected_paths.iter().cloned().collect(),
                fields: HashMap::new(),
                cover: None,
            });
        pending.cover = Some(cover);
    }

    fn save_batch(&mut self, sender: ComponentSender<Self>) {
        if self.batch_save_in_progress {
            return;
        }
        let Some(pending) = self.pending_batch_save.take() else {
            return;
        };
        if pending.is_empty() || pending.paths.len() < 2 {
            return;
        }
        if pending.fields.iter().any(|(&field, value)| {
            TagDraft::default()
                .with_field(field, value.clone())
                .validation_error(field)
                .is_some()
        }) {
            self.status = "请先修正表单中的无效字段。".into();
            self.pending_batch_save = Some(pending);
            return;
        }
        let Some(root) = self.root_directory.clone() else {
            return;
        };
        self.batch_save_in_progress = true;
        self.status = format!("正在批量保存 {} 个文件…", pending.paths.len());
        sender.spawn_oneshot_command(move || {
            let mut files = Vec::new();
            let mut snapshots = Vec::new();
            let result = pending.paths.iter().cloned().try_for_each(|path| {
                let file = match read_audio_file(path.clone(), root.clone()) {
                    Ok(file) => file,
                    Err(error) => {
                        rollback_history_batch(&snapshots)?;
                        return Err(error);
                    }
                };
                let mut draft = TagDraft::from_audio_file(&file);
                for (field, value) in &pending.fields {
                    draft.set(*field, value.clone());
                }
                if let Some(cover) = &pending.cover {
                    draft.cover = cover.clone();
                }
                let snapshot = match create_backup(&root, &path) {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        rollback_history_batch(&snapshots)?;
                        return Err(error);
                    }
                };
                snapshots.push((path.clone(), snapshot));
                if let Err(error) = write_draft(&path, &draft) {
                    rollback_history_batch(&snapshots)?;
                    return Err(error);
                }
                match read_audio_file(path.clone(), root.clone()) {
                    Ok(file) => {
                        files.push(file);
                        Ok(())
                    }
                    Err(error) => {
                        rollback_history_batch(&snapshots)?;
                        Err(error)
                    }
                }
            });
            CmdMsg::BatchSaveFinished {
                result: Box::new(result.map(|_| files)),
                batch: HistoryBatch { snapshots },
                pending,
            }
        });
    }

    fn save_current(&mut self, sender: ComponentSender<Self>) {
        if self.save_in_progress {
            return;
        }
        let Some(mut pending) = self.pending_save.take() else {
            return;
        };
        pending.source = None;
        if !self.active_draft.is_valid() {
            self.status = "请先修正表单中的无效字段。".into();
            self.pending_save = Some(pending);
            self.quitting = false;
            return;
        }
        let (Some(file), Some(root)) = (&self.selected_file, &self.root_directory) else {
            return;
        };
        if pending.path != file.path {
            return;
        }
        let source = file.path.clone();
        let root = root.clone();
        let draft = self.active_draft.clone();
        self.save_in_progress = true;
        self.status = "正在保存标签…".into();
        sender.spawn_oneshot_command(move || {
            let result = create_backup(&root, &source).and_then(|snapshot| {
                write_draft(&source, &draft)
                    .and_then(|_| read_audio_file(source.clone(), root))
                    .map(|file| (file, snapshot))
            });
            match result {
                Ok((file, snapshot)) => CmdMsg::SaveFinished {
                    result: Box::new(Ok(file)),
                    snapshot: Some(snapshot),
                    draft: draft.clone(),
                    pending,
                },
                Err(error) => CmdMsg::SaveFinished {
                    result: Box::new(Err(error)),
                    snapshot: None,
                    draft,
                    pending,
                },
            }
        });
    }

    fn finish_pending_action(&mut self, sender: ComponentSender<Self>) {
        match self.pending_action.take() {
            Some(PendingAction::Select { path, modifiers }) => {
                sender.input(AppMsg::SelectAudioFile { path, modifiers })
            }
            Some(PendingAction::OpenDirectory(path)) => sender.input(AppMsg::DirectoryChosen(path)),
            Some(PendingAction::Undo) => sender.input(AppMsg::Undo),
            Some(PendingAction::Redo) => sender.input(AppMsg::Redo),
            None => {}
        }
    }

    fn show_close_dialog(&mut self, sender: ComponentSender<Self>, root: &gtk::Window) {
        if self.close_dialog_open {
            return;
        }
        self.close_dialog_open = true;
        let dialog = adw::AlertDialog::builder()
            .heading("保存更改？")
            .body("关闭前有尚未保存的标签或封面更改。")
            .prefer_wide_layout(true)
            .close_response("cancel")
            .build();
        dialog.add_responses(&[("cancel", "取消"), ("discard", "不保存"), ("save", "保存")]);
        dialog.set_response_appearance("discard", adw::ResponseAppearance::Destructive);
        dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("save"));
        let dialog_sender = sender.clone();
        dialog.connect_response(None, move |_, response| {
            let action = match response {
                "discard" => CloseAction::Discard,
                "save" => CloseAction::Save,
                _ => CloseAction::Cancel,
            };
            dialog_sender.input(AppMsg::CloseAction(action));
        });
        dialog.present(Some(root));
    }

    fn discard_changes_and_close(&mut self, sender: ComponentSender<Self>, root: &gtk::Window) {
        if let Some(pending) = self.pending_save.take()
            && let Some(source) = pending.source
        {
            source.remove();
        }
        self.pending_batch_save = None;
        self.quitting = true;
        self.finish_close(sender, root);
    }

    fn finish_close(&mut self, sender: ComponentSender<Self>, root: &gtk::Window) {
        if let Some(directory) = self.root_directory.clone() {
            sender.spawn_oneshot_command(move || CmdMsg::BackupsCleared(clear_backups(&directory)));
        } else {
            root.destroy();
        }
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
        rendered_draft_revision: Cell<u64>,
        sidebar_button: gtk::ToggleButton,
        inspector_button: gtk::ToggleButton,
        syncing: Rc<Cell<bool>>,
        status_label: gtk::Label,
        undo_button: gtk::Button,
        redo_button: gtk::Button,
        save_button: gtk::Button,
        batch_save_warning: gtk::Image,
        editor_cover: Rc<RefCell<EditorCoverTransition>>,
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
                        #[watch]
                        set_sensitive: !model.is_saving(),
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
                        gtk::Overlay {
                            set_hexpand: true,
                            set_vexpand: true,
                            set_width_request: 480,
                            add_overlay = &gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 8,
                            set_hexpand: true,
                            set_width_request: 480,
                            set_margin_all: 20,
                            #[watch]
                            set_visible: model.selected_file.is_some(),
                            #[watch]
                            set_sensitive: !model.is_saving(),
                            gtk::Label {
                                #[watch]
                                set_label: &model.selection_summary(),
                                set_halign: gtk::Align::Start,
                                add_css_class: "title-4",
                            },
                            gtk::Label {
                                set_label: "编辑任一字段会仅更新该字段，并应用到所有已选文件。",
                                set_halign: gtk::Align::Start,
                                add_css_class: "dim-label",
                                #[watch]
                                set_visible: model.is_batch_editing(),
                            },
                            gtk::Label { set_label: " ", #[watch] set_visible: !model.is_batch_editing() },
                            gtk::Label { set_label: "标题", set_halign: gtk::Align::Start },
                            #[name = "title_entry"]
                            gtk::Entry {
                                #[watch]
                                set_placeholder_text: Some(model.field_placeholder(TagField::Title)),
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
                                #[watch]
                                set_placeholder_text: Some(model.field_placeholder(TagField::Artist)),
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::Artist, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::Artist).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::Artist).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "专辑", set_halign: gtk::Align::Start },
                            #[name = "album_entry"]
                            gtk::Entry {
                                #[watch]
                                set_placeholder_text: Some(model.field_placeholder(TagField::Album)),
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::Album, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::Album).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::Album).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "专辑艺人", set_halign: gtk::Align::Start },
                            #[name = "album_artist_entry"]
                            gtk::Entry {
                                #[watch]
                                set_placeholder_text: Some(model.field_placeholder(TagField::AlbumArtist)),
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::AlbumArtist, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::AlbumArtist).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::AlbumArtist).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "年份", set_halign: gtk::Align::Start },
                            #[name = "year_entry"]
                            gtk::Entry {
                                #[watch]
                                set_placeholder_text: Some(model.field_placeholder(TagField::Year)),
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::Year, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::Year).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::Year).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "曲目号", set_halign: gtk::Align::Start },
                            #[name = "track_entry"]
                            gtk::Entry {
                                #[watch]
                                set_placeholder_text: Some(model.field_placeholder(TagField::TrackNumber)),
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::TrackNumber, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::TrackNumber).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::TrackNumber).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "碟号", set_halign: gtk::Align::Start },
                            #[name = "disc_entry"]
                            gtk::Entry {
                                #[watch]
                                set_placeholder_text: Some(model.field_placeholder(TagField::DiscNumber)),
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::DiscNumber, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::DiscNumber).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::DiscNumber).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },
                            gtk::Label { set_label: "流派", set_halign: gtk::Align::Start },
                            #[name = "genre_entry"]
                            gtk::Entry {
                                #[watch]
                                set_placeholder_text: Some(model.field_placeholder(TagField::Genre)),
                                connect_changed[sender, syncing] => move |entry| {
                                    if !syncing.get() { sender.input(AppMsg::SetField(TagField::Genre, entry.text().to_string())); }
                                },
                            },
                            gtk::Label { #[watch] set_label: model.active_draft.validation_error(TagField::Genre).unwrap_or(""), #[watch] set_visible: model.active_draft.validation_error(TagField::Genre).is_some(), add_css_class: "error", set_halign: gtk::Align::Start },

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
                        set_sensitive: model.selected_file.is_some() && !model.is_saving(),
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
            selected_paths: BTreeSet::new(),
            mixed_fields: std::collections::HashSet::new(),
            covers_mixed: false,
            selection_anchor: None,
            sidebar_visible: false,
            inspector_visible: false,
            original_draft: None,
            active_draft: TagDraft::default(),
            saved_drafts: HashMap::new(),
            status: "选择一个音乐目录以开始浏览。".into(),
            cover_error: None,
            tree_revision: 0,
            selection_revision: 0,
            batch_draft_revision: 0,
            tree_rows: FactoryVecDeque::builder()
                .launch(
                    gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(2)
                        .build(),
                )
                .forward(sender.input_sender(), AppMsg::TreeRow),
            album_cover_textures: album_cover_textures.clone(),
            blurred_cover_cache: RefCell::new(BlurredCoverCache::default()),
            cover_revision: 0,
            history: BatchHistory::default(),
            pending_save: None,
            pending_batch_save: None,
            save_in_progress: false,
            batch_save_in_progress: false,
            pending_action: None,
            quitting: false,
            close_dialog_open: false,
            draft_revision: 0,
        };
        let syncing = Rc::new(Cell::new(false));
        let rendered_cover_revision = Cell::new(u64::MAX);
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
            .sensitive(false)
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
            .sensitive(false)
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
             .tree-thumbnail, .album-thumbnail { min-width: 24px; min-height: 24px; border-radius: 4px; }\
             .editor-cover-background { opacity: 0.2; }\
             .batch-save-warning { color: #e5a50a; }",
        );
        if let Some(display) = gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &style_provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let undo_button = gtk::Button::builder()
            .icon_name("edit-undo-symbolic")
            .tooltip_text("撤销（⌘Z / Ctrl+Z）")
            .sensitive(false)
            .build();
        let undo_sender = sender.clone();
        undo_button.connect_clicked(move |_| undo_sender.input(AppMsg::Undo));
        header_bar.pack_end(&undo_button);

        let redo_button = gtk::Button::builder()
            .icon_name("edit-redo-symbolic")
            .tooltip_text("重做（⇧⌘Z / Ctrl+Shift+Z）")
            .sensitive(false)
            .build();
        let redo_sender = sender.clone();
        redo_button.connect_clicked(move |_| redo_sender.input(AppMsg::Redo));
        header_bar.pack_end(&redo_button);

        let save_button = gtk::Button::builder()
            .icon_name("document-save-symbolic")
            .tooltip_text("保存")
            .sensitive(false)
            .build();
        let save_sender = sender.clone();
        save_button.connect_clicked(move |_| save_sender.input(AppMsg::SaveNow));
        header_bar.pack_end(&save_button);

        let batch_save_warning = gtk::Image::builder()
            .icon_name("dialog-warning-symbolic")
            .tooltip_text("批量编辑尚未保存")
            .visible(false)
            .build();
        batch_save_warning.add_css_class("batch-save-warning");
        header_bar.pack_end(&batch_save_warning);

        let status_label = gtk::Label::builder()
            .label(model.header_title())
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();
        header_bar.set_title_widget(Some(&status_label));
        root.set_titlebar(Some(&header_bar));

        let tree_box = model.tree_rows.widget();
        let editor_cover = Rc::new(RefCell::new(EditorCoverTransition::default()));
        let editor_cover_background = gtk::DrawingArea::new();
        editor_cover_background.set_can_target(false);
        editor_cover_background.add_css_class("editor-cover-background");
        let cover_for_draw = editor_cover.clone();
        editor_cover_background.set_draw_func(move |_, context, width, height| {
            let transition = cover_for_draw.borrow();
            let progress = transition_progress(&transition);
            match (&transition.previous, &transition.current) {
                (Some(previous), Some(current)) => {
                    draw_cover(context, &previous.pixbuf, width, height, 1.0);
                    draw_cover(context, &current.pixbuf, width, height, progress);
                }
                (Some(previous), None) => {
                    draw_cover(context, &previous.pixbuf, width, height, 1.0 - progress);
                }
                (None, Some(current)) => {
                    draw_cover(context, &current.pixbuf, width, height, progress);
                }
                (None, None) => {}
            }
        });
        let widgets = view_output!();
        widgets.editor.set_child(Some(&editor_cover_background));

        configure_macos_window(&root);
        configure_macos_window_style();
        configure_macos_menubar(&root, sender.clone());

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

        let key_controller = gtk::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let key_sender = sender.clone();
        key_controller.connect_key_pressed(move |_, key, _, modifiers| {
            let primary = modifiers.contains(gdk::ModifierType::CONTROL_MASK)
                || modifiers.contains(gdk::ModifierType::META_MASK);
            if !primary || key != gdk::Key::z && key != gdk::Key::Z {
                return glib::Propagation::Proceed;
            }
            if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
                key_sender.input(AppMsg::Redo);
            } else {
                key_sender.input(AppMsg::Undo);
            }
            glib::Propagation::Stop
        });
        root.add_controller(key_controller);

        let close_sender = sender.clone();
        root.connect_close_request(move |_| {
            close_sender.input(AppMsg::RequestClose);
            glib::Propagation::Stop
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: AppMsg, sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            AppMsg::ChooseDirectory => choose_directory(root, sender),
            AppMsg::DirectoryChosen(path) => {
                if self.is_saving() {
                    self.status = "正在保存标签，请稍后再切换目录。".into();
                    return;
                }
                if self.pending_batch_save.is_some() {
                    self.status = "批量编辑尚未保存，请先点击保存按钮。".into();
                    return;
                }
                if self.pending_save.is_some() || self.save_in_progress {
                    self.pending_action = Some(PendingAction::OpenDirectory(path));
                    self.save_current(sender);
                    return;
                }
                self.sidebar_visible = true;
                self.status = format!("正在扫描 {}…", path.display());
                self.root_directory = Some(path.clone());
                self.tree = None;
                self.expanded_paths.clear();
                self.history = BatchHistory::default();
                self.tree_revision = self.tree_revision.wrapping_add(1);
                self.clear_selection();
                let revision = self.tree_revision;
                sender.spawn_oneshot_command(move || CmdMsg::DirectoryScanned {
                    result: scan_directory(path.clone()),
                    path,
                    revision,
                });
            }

            AppMsg::SelectAudioFile { path, modifiers } => {
                if self.is_saving() {
                    self.status = "正在保存标签，请稍后再选择文件。".into();
                    return;
                }
                if self.pending_batch_save.is_some() {
                    self.status = "批量编辑尚未保存，请先点击保存按钮。".into();
                    return;
                }
                if self.selected_path.as_deref() != Some(path.as_path())
                    && (self.pending_save.is_some() || self.save_in_progress)
                {
                    self.pending_action = Some(PendingAction::Select { path, modifiers });
                    self.save_current(sender);
                    return;
                }
                if !self.select_audio_file(path, modifiers) {
                    return;
                }
                self.selection_revision = self.selection_revision.wrapping_add(1);
                if self.is_batch_editing() {
                    self.load_batch_drafts(sender.clone());
                }
                let revision = self.selection_revision;
                let Some(path) = self.selected_path.clone() else {
                    return;
                };
                let Some(root_path) = self.root_directory.clone() else {
                    return;
                };
                self.status = format!("正在读取 {}…", path.display());
                sender.spawn_oneshot_command(move || CmdMsg::AudioLoaded {
                    result: Box::new(read_audio_file(path.clone(), root_path)),
                    path,
                    revision,
                });
            }
            AppMsg::SetSidebarVisible(visible) => self.sidebar_visible = visible,
            AppMsg::SetInspectorVisible(visible) => {
                self.inspector_visible = visible && self.selected_file.is_some();
            }
            AppMsg::ToggleSidebar => self.sidebar_visible = !self.sidebar_visible,
            AppMsg::ToggleInspector => {
                if self.selected_file.is_some() {
                    self.inspector_visible = !self.inspector_visible;
                }
            }
            AppMsg::TreeRow(output) => match output {
                ui::tree_row::TreeRowOutput::ToggleDirectory(path) => self.toggle_directory(&path),
                ui::tree_row::TreeRowOutput::SelectAudioFile { path, modifiers } => {
                    sender.input(AppMsg::SelectAudioFile { path, modifiers });
                }
            },
            AppMsg::SetField(field, value) => {
                if self.is_batch_editing() {
                    self.batch_draft_revision = self.batch_draft_revision.wrapping_add(1);
                    self.active_draft.set(field, value.clone());
                    self.mixed_fields.remove(&field);
                    self.draft_revision = self.draft_revision.wrapping_add(1);
                    self.stage_batch_field(field, value);
                } else {
                    self.active_draft.set(field, value);
                    self.schedule_save(sender);
                }
            }
            AppMsg::SaveNow => {
                if self.is_batch_editing() {
                    self.save_batch(sender);
                } else {
                    self.save_current(sender);
                }
            }
            AppMsg::Undo | AppMsg::Redo => {
                if self.is_saving() || self.pending_batch_save.is_some() {
                    self.status = "保存完成前无法撤销或重做。".into();
                    return;
                }
                let is_undo = matches!(msg, AppMsg::Undo);
                if self.pending_save.is_some() || self.save_in_progress {
                    self.pending_action = Some(if is_undo {
                        PendingAction::Undo
                    } else {
                        PendingAction::Redo
                    });
                    self.save_current(sender);
                    return;
                }
                let Some(root) = self.root_directory.clone() else {
                    return;
                };
                let Some(batch) = self.history.take(&self.selected_paths, is_undo) else {
                    self.status = "当前选择与最近的批量历史不匹配。".into();
                    return;
                };
                self.batch_save_in_progress = true;
                self.status = if is_undo {
                    format!("正在撤销 {} 个文件…", batch.snapshots.len())
                } else {
                    format!("正在重做 {} 个文件…", batch.snapshots.len())
                };
                sender.spawn_oneshot_command(move || {
                    let result = restore_history_batch(&root, &batch);
                    CmdMsg::HistoryBatchRestored {
                        batch,
                        result: Box::new(result),
                        is_undo,
                    }
                });
            }
            AppMsg::RequestClose => {
                if self.is_saving() {
                    self.status = "正在保存标签，完成后才能关闭窗口。".into();
                } else if self.has_pending_save() {
                    self.show_close_dialog(sender, root);
                } else {
                    self.discard_changes_and_close(sender, root);
                }
            }
            AppMsg::CloseAction(action) => {
                self.close_dialog_open = false;
                match action {
                    CloseAction::Cancel => {}
                    CloseAction::Discard => self.discard_changes_and_close(sender, root),
                    CloseAction::Save => {
                        self.quitting = true;
                        if self.is_batch_editing() {
                            self.save_batch(sender);
                        } else {
                            self.save_current(sender);
                        }
                    }
                }
            }
            AppMsg::ShowAbout => {
                adw::AboutDialog::builder()
                    .application_icon("com.github.anson2251.sleeve")
                    .application_name("Sleeve")
                    .comments("音频标签与封面编辑器")
                    .copyright("© 2026 Anson2251")
                    .developer_name("Anson2251")
                    .license_type(gtk::License::Custom)
                    .license(
                        "This program is free software: you can redistribute it and/or modify it under the terms of the GNU General Public License as published by the Free Software Foundation, either version 3 of the License, or (at your option) any later version.\n\nThis program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more details.\n\nYou should have received a copy of the GNU General Public License along with this program.  If not, see <a href=\"https://www.gnu.org/licenses/\">https://www.gnu.org/licenses/</a>.",
                    )
                    .website("https://github.com/anson2251/sleeve")
                    .build()
                    .present(Some(root));
            }
            AppMsg::ChooseCover => choose_cover(root, sender),
            AppMsg::CoverChosen(path) => {
                if image::ImageReader::open(&path)
                    .ok()
                    .and_then(|reader| reader.with_guessed_format().ok())
                    .and_then(|reader| reader.decode().ok())
                    .is_some()
                {
                    let cover = CoverDraft::External(path);
                    self.cover_error = None;
                    if self.is_batch_editing() {
                        self.batch_draft_revision = self.batch_draft_revision.wrapping_add(1);
                        self.active_draft.cover = cover.clone();
                        self.covers_mixed = false;
                        self.cover_revision = self.cover_revision.wrapping_add(1);
                        self.stage_batch_cover(cover);
                    } else {
                        self.active_draft.cover = cover;
                        self.cover_revision = self.cover_revision.wrapping_add(1);
                        self.schedule_save(sender);
                    }
                } else {
                    self.cover_error = Some("请选择有效的图片文件。".into());
                }
            }
            AppMsg::RemoveCover => {
                if self.is_batch_editing() {
                    self.batch_draft_revision = self.batch_draft_revision.wrapping_add(1);
                    self.active_draft.cover = CoverDraft::Removed;
                    self.covers_mixed = false;
                    self.cover_revision = self.cover_revision.wrapping_add(1);
                    self.stage_batch_cover(CoverDraft::Removed);
                } else {
                    self.active_draft.cover = CoverDraft::Removed;
                    self.cover_revision = self.cover_revision.wrapping_add(1);
                    self.schedule_save(sender);
                }
            }
        }
    }

    fn update_cmd(&mut self, msg: CmdMsg, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            CmdMsg::DirectoryScanned {
                result,
                path,
                revision,
            } => {
                if revision != self.tree_revision
                    || self.root_directory.as_deref() != Some(path.as_path())
                {
                    return;
                }
                match result {
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
                }
            }
            CmdMsg::AudioLoaded {
                result,
                path,
                revision,
            } => {
                if revision != self.selection_revision
                    || self.selected_path.as_deref() != Some(path.as_path())
                {
                    return;
                }
                match *result {
                    Ok(file) => self.load_file(file, "正在编辑 {}。"),
                    Err(error) => {
                        self.clear_selection();
                        self.status = error;
                    }
                }
            }
            CmdMsg::SaveFinished {
                result,
                snapshot,
                draft,
                pending,
            } => {
                self.save_in_progress = false;
                match *result {
                    Ok(file) => {
                        if let Some(snapshot) = snapshot {
                            self.history.record_save(HistoryBatch {
                                snapshots: vec![(file.path.clone(), snapshot)],
                            });
                        }
                        if self.active_draft == draft {
                            self.load_file(file, "已自动保存。");
                        } else {
                            self.status = "已自动保存，正在等待后续修改。".into();
                        }
                        if self.pending_save.is_some() {
                            self.save_current(sender);
                        } else if self.quitting {
                            self.finish_close(sender, _root);
                        } else {
                            self.finish_pending_action(sender);
                        }
                    }
                    Err(error) => {
                        self.pending_save = Some(pending);
                        self.status = error;
                        self.quitting = false;
                    }
                }
            }
            CmdMsg::BatchDraftsLoaded {
                paths,
                drafts,
                selection_revision,
                batch_draft_revision,
            } => {
                if is_current_batch_draft_result(
                    &paths,
                    selection_revision,
                    batch_draft_revision,
                    &self.selected_paths,
                    self.selection_revision,
                    self.batch_draft_revision,
                ) && !drafts.is_empty()
                {
                    let (draft, mixed_fields, covers_mixed) = common_draft(&drafts);
                    self.active_draft = draft;
                    self.mixed_fields = mixed_fields;
                    self.covers_mixed = covers_mixed;
                    self.cover_revision = self.cover_revision.wrapping_add(1);
                    self.draft_revision = self.draft_revision.wrapping_add(1);
                }
            }
            CmdMsg::BatchSaveFinished {
                result,
                batch,
                pending,
            } => {
                self.batch_save_in_progress = false;
                match *result {
                    Ok(files) => {
                        let saved = files.len();
                        self.history.record_save(batch);
                        if let Some(file) = files
                            .into_iter()
                            .find(|file| self.selected_path.as_deref() == Some(file.path.as_path()))
                        {
                            self.selected_file = Some(file);
                        }
                        self.status = format!("已批量保存 {saved} 个文件。");
                        if self.quitting {
                            self.finish_close(sender, _root);
                        } else {
                            self.load_batch_drafts(sender);
                        }
                    }
                    Err(error) => {
                        self.pending_batch_save = Some(pending);
                        self.status = error;
                        self.quitting = false;
                    }
                }
            }
            CmdMsg::HistoryBatchRestored {
                batch,
                result,
                is_undo,
            } => {
                self.batch_save_in_progress = false;
                match *result {
                    Ok((files, current_batch)) => {
                        let restored = files.len();
                        self.history.complete_restore(current_batch, is_undo);
                        if let Some(file) = files
                            .into_iter()
                            .find(|file| self.selected_path.as_deref() == Some(file.path.as_path()))
                        {
                            self.selected_file = Some(file);
                        }
                        self.status = if is_undo {
                            format!("已撤销 {restored} 个文件。")
                        } else {
                            format!("已重做 {restored} 个文件。")
                        };
                        self.load_batch_drafts(sender);
                    }
                    Err(error) => {
                        self.history.restore_failed(batch, is_undo);
                        self.status = error;
                    }
                }
            }
            CmdMsg::BackupsCleared(result) => match result {
                Ok(()) => _root.destroy(),
                Err(error) => {
                    self.status = error;
                    self.quitting = false;
                }
            },
        }
    }

    fn post_view() {
        status_label.set_label(&model.header_title());
        sidebar_button.set_sensitive(model.root_directory.is_some());
        sidebar_button.set_active(model.sidebar_visible);
        inspector_button.set_sensitive(model.selected_file.is_some() && !model.is_saving());
        inspector_button.set_active(model.inspector_visible);
        save_button.set_sensitive(model.has_pending_save() && !model.is_saving());
        batch_save_warning
            .set_visible(model.is_batch_editing() && model.pending_batch_save.is_some());

        if rendered_cover_revision.replace(model.cover_revision) != model.cover_revision {
            cover_dimensions.set_label(&update_cover(cover, &model.active_draft.cover));
            update_cover_background(
                editor,
                editor_cover,
                &model.blurred_cover_cache,
                &model.active_draft.cover,
            );
        }
        let can_undo = model.history.can_undo(&model.selected_paths);
        let can_redo = model.history.can_redo(&model.selected_paths);
        undo_button.set_sensitive(can_undo && !model.is_saving());
        redo_button.set_sensitive(can_redo && !model.is_saving());
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canceling_close_keeps_pending_changes() {
        let pending = PendingBatchSave {
            paths: vec![PathBuf::from("track.flac")],
            fields: HashMap::from([(TagField::Artist, "Changed".into())]),
            cover: None,
        };

        assert!(!pending.is_empty());
    }
}

#[cfg(target_os = "macos")]
use std::sync::OnceLock;

#[cfg(target_os = "macos")]
static MACOS_MENU_CALLBACK: OnceLock<Box<dyn Fn(AppMsg) + Send + Sync>> = OnceLock::new();

#[cfg(target_os = "macos")]
static MACOS_MENU_TARGET: OnceLock<objc2::rc::Retained<SleeveMenuHandler>> = OnceLock::new();

#[cfg(target_os = "macos")]
objc2::define_class!(
    #[unsafe(super(objc2::runtime::NSObject))]
    #[name = "SleeveMenuHandler"]
    struct SleeveMenuHandler;

    impl SleeveMenuHandler {
        #[unsafe(method(handleMenuAction:))]
        fn handle_menu_action(&self, sender: &objc2::runtime::NSObject) {
            use objc2::msg_send;

            let tag: isize = unsafe { msg_send![sender, tag] };
            let message = match tag {
                1 => AppMsg::ShowAbout,
                2 => AppMsg::ChooseDirectory,
                3 => AppMsg::Undo,
                4 => AppMsg::Redo,
                5 => AppMsg::ToggleSidebar,
                6 => AppMsg::ToggleInspector,
                7 => AppMsg::RequestClose,
                _ => return,
            };
            if let Some(callback) = MACOS_MENU_CALLBACK.get() {
                callback(message);
            }
        }
    }
);

#[cfg(target_os = "macos")]
impl SleeveMenuHandler {
    objc2::extern_methods!(
        #[unsafe(method(new))]
        fn new() -> objc2::rc::Retained<Self>;
    );
}

#[cfg(target_os = "macos")]
fn configure_macos_menubar(_: &gtk::Window, sender: ComponentSender<AppModel>) {
    use objc2::{MainThreadMarker, sel};
    use objc2_app_kit::{NSApp, NSEventModifierFlags, NSMenu, NSMenuItem};

    let menu_sender = sender.clone();
    let (tx, rx) = std::sync::mpsc::channel::<AppMsg>();
    let _ = MACOS_MENU_CALLBACK.set(Box::new(move |message| {
        let _ = tx.send(message);
    }));
    glib::timeout_add_local(Duration::from_millis(50), move || {
        while let Ok(message) = rx.try_recv() {
            menu_sender.input(message);
        }
        glib::ControlFlow::Continue
    });

    let _ = MACOS_MENU_TARGET.set(SleeveMenuHandler::new());
    let target = MACOS_MENU_TARGET
        .get()
        .expect("macOS menu target should be initialized");
    let mtm = unsafe { MainThreadMarker::new_unchecked() };

    unsafe {
        let main_menu = NSMenu::init(mtm.alloc::<NSMenu>());
        let app_menu_item = NSMenuItem::init(mtm.alloc::<NSMenuItem>());
        let app_menu = NSMenu::init(mtm.alloc::<NSMenu>());
        app_menu_item.setSubmenu(Some(&app_menu));
        main_menu.addItem(&app_menu_item);

        add_macos_callback_item(&app_menu, mtm, target, "关于 Sleeve", 1, None);
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        add_macos_responder_item(
            &app_menu,
            mtm,
            "隐藏 Sleeve",
            sel!(hide:),
            "h",
            NSEventModifierFlags::Command,
        );
        add_macos_responder_item(
            &app_menu,
            mtm,
            "隐藏其他",
            sel!(hideOtherApplications:),
            "h",
            NSEventModifierFlags::Command | NSEventModifierFlags::Option,
        );
        add_macos_responder_item(
            &app_menu,
            mtm,
            "全部显示",
            sel!(unhideAllApplications:),
            "",
            NSEventModifierFlags::empty(),
        );
        app_menu.addItem(&NSMenuItem::separatorItem(mtm));
        add_macos_callback_item(
            &app_menu,
            mtm,
            target,
            "退出 Sleeve",
            7,
            Some(("q", NSEventModifierFlags::Command)),
        );

        let file_menu = add_macos_submenu(&main_menu, mtm, "文件");
        add_macos_callback_item(
            &file_menu,
            mtm,
            target,
            "打开目录…",
            2,
            Some(("o", NSEventModifierFlags::Command)),
        );

        let edit_menu = add_macos_submenu(&main_menu, mtm, "编辑");
        add_macos_callback_item(
            &edit_menu,
            mtm,
            target,
            "撤销",
            3,
            Some(("z", NSEventModifierFlags::Command)),
        );
        add_macos_callback_item(
            &edit_menu,
            mtm,
            target,
            "重做",
            4,
            Some((
                "z",
                NSEventModifierFlags::Command | NSEventModifierFlags::Shift,
            )),
        );
        edit_menu.addItem(&NSMenuItem::separatorItem(mtm));
        add_macos_responder_item(
            &edit_menu,
            mtm,
            "剪切",
            sel!(cut:),
            "x",
            NSEventModifierFlags::Command,
        );
        add_macos_responder_item(
            &edit_menu,
            mtm,
            "复制",
            sel!(copy:),
            "c",
            NSEventModifierFlags::Command,
        );
        add_macos_responder_item(
            &edit_menu,
            mtm,
            "粘贴",
            sel!(paste:),
            "v",
            NSEventModifierFlags::Command,
        );
        edit_menu.addItem(&NSMenuItem::separatorItem(mtm));
        add_macos_responder_item(
            &edit_menu,
            mtm,
            "全选",
            sel!(selectAll:),
            "a",
            NSEventModifierFlags::Command,
        );

        let view_menu = add_macos_submenu(&main_menu, mtm, "显示");
        add_macos_callback_item(&view_menu, mtm, target, "显示/隐藏文件列表", 5, None);
        add_macos_callback_item(&view_menu, mtm, target, "显示/隐藏检查器", 6, None);

        NSApp(mtm).setMainMenu(Some(&main_menu));
    }
}

#[cfg(target_os = "macos")]
unsafe fn add_macos_submenu(
    main_menu: &objc2_app_kit::NSMenu,
    mtm: objc2::MainThreadMarker,
    title: &str,
) -> objc2::rc::Retained<objc2_app_kit::NSMenu> {
    use objc2_app_kit::{NSMenu, NSMenuItem};
    use objc2_foundation::NSString;

    let item = NSMenuItem::init(mtm.alloc::<NSMenuItem>());
    let menu = NSMenu::init(mtm.alloc::<NSMenu>());
    menu.setTitle(&NSString::from_str(title));
    item.setSubmenu(Some(&menu));
    main_menu.addItem(&item);
    menu
}

#[cfg(target_os = "macos")]
unsafe fn add_macos_callback_item(
    menu: &objc2_app_kit::NSMenu,
    mtm: objc2::MainThreadMarker,
    target: &SleeveMenuHandler,
    title: &str,
    tag: isize,
    shortcut: Option<(&str, objc2_app_kit::NSEventModifierFlags)>,
) {
    use objc2::sel;
    use objc2_app_kit::NSMenuItem;
    use objc2_foundation::NSString;

    let item = NSMenuItem::init(mtm.alloc::<NSMenuItem>());
    item.setTitle(&NSString::from_str(title));
    unsafe {
        item.setAction(Some(sel!(handleMenuAction:)));
        item.setTarget(Some(target));
    }
    item.setTag(tag);
    if let Some((key, modifiers)) = shortcut {
        item.setKeyEquivalent(&NSString::from_str(key));
        item.setKeyEquivalentModifierMask(modifiers);
    }
    menu.addItem(&item);
}

#[cfg(target_os = "macos")]
unsafe fn add_macos_responder_item(
    menu: &objc2_app_kit::NSMenu,
    mtm: objc2::MainThreadMarker,
    title: &str,
    action: objc2::runtime::Sel,
    key: &str,
    modifiers: objc2_app_kit::NSEventModifierFlags,
) {
    use objc2_app_kit::NSMenuItem;
    use objc2_foundation::NSString;

    let item = NSMenuItem::init(mtm.alloc::<NSMenuItem>());
    item.setTitle(&NSString::from_str(title));
    unsafe {
        item.setAction(Some(action));
        item.setTarget(None);
    }
    item.setKeyEquivalent(&NSString::from_str(key));
    item.setKeyEquivalentModifierMask(modifiers);
    menu.addItem(&item);
}

#[cfg(not(target_os = "macos"))]
fn configure_macos_menubar(_: &gtk::Window, _: ComponentSender<AppModel>) {}

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
