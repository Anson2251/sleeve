# Audio Tag & Cover Editor Design

## Goal

Build a desktop audio metadata editor with Relm4 and GTK4. A user opens a directory, browses supported audio files in a recursive tree, selects one file, edits its tag fields and cover, then safely writes changes back after automatically creating a versioned backup.

## Scope

### Included

- Native directory selection.
- Recursive discovery of supported audio files.
- Tree-shaped browser showing only directories that contain supported audio files and supported audio files themselves.
- Selecting an audio file reads its tags, embedded cover, and available read-only metadata through `audiotags`.
- In-memory editing for title, artist, album, album artist, year, track number, disc number, and genre.
- Cover preview from embedded artwork, a selected image file, or an image dropped onto the cover area.
- Restore the selected item’s editable draft from the initially loaded values.
- Keep each selected file’s saved draft available during the running application session.
- Protected single-file save: create a versioned backup before writing any tags or artwork to the original audio file.
- Restore a selected audio file from a versioned backup through a HeaderBar popover.
- Clear empty states and errors for invalid directories, unreadable media, invalid cover images, backup failures, and write failures.

### Excluded

- Batch editing.
- Audio playback.
- Persistent drafts across application launches.
- Displaying non-audio files.

## Layout and Interaction

The main window uses a three-column horizontal layout.

1. **Left: directory and audio tree**
   - A button opens the GTK directory chooser.
   - The chosen directory is the root node.
   - Directories are expandable and collapsible.
   - A directory click only changes its expansion state.
   - An audio file click loads it in the other two columns.
   - File names are shown under their parent directory. Directories with no supported audio descendants are omitted.

2. **Center: tag editor**
   - The selected file’s name and relative path are displayed read-only.
   - Editable fields: title, artist, album, album artist, year, track number, disc number, and genre.
   - Editing any field changes only the active in-memory `TagDraft`.
   - **保存** validates the draft, creates a backup, and writes its tags/artwork to the original audio file in the background.
   - **还原草稿** replaces the active draft with the values last read from the source file, without writing.
   - After a successful save or restore-from-backup, the file is re-read and the loaded values become the new original draft.
   - No file selected presents an instructional empty state.

3. **HeaderBar: primary actions**
   - **打开目录** remains in the HeaderBar.
   - **恢复备份** is placed at the HeaderBar right edge. It is disabled when no audio file is selected or no versioned backup exists for that file.
   - Clicking it opens a `gtk::Popover` listing backups for the current file in descending timestamp order. Each entry shows timestamp and backup file size, with an explicit restore action.

4. **Right: metadata and artwork**
   - Shows read-only metadata returned by the parser when available (format, duration, bitrate, sample rate, channels, file size).
   - Shows embedded artwork if it can be decoded.
   - The cover area accepts image-file drops and provides a file-picker action as an alternative.
   - Only valid image files are accepted. A replacement changes the current draft preview only.
   - **Remove cover** clears the cover from the active in-memory draft only.

## Architecture

```text
src/
├── main.rs                 # RelmApp construction and root component launch
├── app.rs                  # AppModel, AppMsg, view!, background-result handling
├── models/
│   ├── mod.rs
│   ├── audio_file.rs       # AudioFile, AudioMetadata, parsing/error status
│   ├── file_tree.rs        # FileTreeNode and filtering/tree construction
│   └── tag_draft.rs        # Editable tag values and cover draft state
├── services/
│   ├── mod.rs
│   ├── directory_scan.rs   # Recursive supported-file discovery
│   └── tag_reader.rs       # audiotags tag, metadata, and artwork extraction
└── ui/
    ├── mod.rs
    ├── file_tree.rs        # GTK TreeListModel/ListView setup helpers
    ├── tag_editor.rs       # Tag-editor view helpers and bindings
    └── cover_panel.rs      # Metadata, cover preview, drop target helpers
```

`AppModel` is the source of truth and coordinates the three panels. The `models` package has no GTK dependencies except types necessary for image handles. `services` performs filesystem traversal and tag parsing. The `ui` package constructs focused widget subtrees and maps user signals to `AppMsg` messages.

## Data Model

- `FileTreeNode`: a directory or audio-file node, with display name, full path, and ordered children. Directory nodes exist only when they contain a supported audio-file descendant.
- `AudioFile`: full and relative paths, initial parsed tags, read-only metadata, and optional embedded-cover data.
- `TagDraft`: optional text values for editable fields, plus `CoverDraft`.
- `CoverDraft`: original embedded image, image selected from a path, an explicit removal state, or no available cover.
- `AppModel`: chosen root directory, tree, selected file path, active source values, active `TagDraft`, session-local draft map, scan/load state, and a user-visible error message where needed.

## Protected Save and Restore

### Versioned backup layout

Every protected operation first copies the current source file to a timestamped, hidden backup directory under the selected root:

```text
<selected-root>/.sleeve-backups/
└── 2026-07-18T14-32-08/
    └── Album/01 - Track.flac
```

The audio file’s relative path is preserved beneath the timestamp directory. This avoids filename collisions and makes a complete save session recoverable. Timestamp values use a filesystem-safe local-time representation (`YYYY-MM-DDTHH-MM-SS`).

### Save

1. Validate the active `TagDraft`; reject invalid fields before any filesystem operation.
2. Confirm an audio file and selected root are available.
3. In a background command, create the timestamped backup parent and copy the source file to the matching relative path.
4. If copying fails, stop and leave the original file unchanged.
5. Apply the editable tags and cover state to the original file through `audiotags` and write it back.
6. On success, re-read the file and replace the active/original draft with persisted values. On failure, retain the in-memory draft and surface an actionable error.

### Restore

1. Locate backups for the selected audio file under `.sleeve-backups`, sorted newest first.
2. The HeaderBar popover presents a version list with timestamp and size.
3. On selecting a version, first create a new timestamped backup of the current source file.
4. Only after the protection backup succeeds, copy the selected backup over the source file.
5. Re-read the restored file and refresh tags, metadata, and artwork. A restore failure preserves the current in-memory draft and reports the cause.

## Data Flow and Responsiveness

1. The directory chooser sends `OpenDirectory`.
2. `AppModel` starts a background command to recursively scan the directory. The UI remains responsive and shows a loading state.
3. The result is converted into a filtered `FileTreeNode` root and rendered using GTK `TreeListModel` and `ListView`.
4. Selecting a file starts a separate background tag-read command. It loads that file’s tags, artwork, and metadata.
5. The result initializes an active `TagDraft`, unless the session-local draft map already contains a saved draft for the file.
6. Field and cover events mutate only the active draft. Save and restore execute their copy/write work in background commands, then re-read the persisted file.
7. All model-derived widget properties use Relm4 `#[watch]` or explicit update methods so the view refreshes after messages.

## File and Image Handling

- Candidate audio extensions are `mp3`, `flac`, `m4a`, `m4b`, and `mp4`, case-insensitively, matching the formats supported by `audiotags`.
- The scanner sorts directories and files case-insensitively for stable tree presentation.
- Traversal skips unreadable filesystem entries while collecting a contextual warning; a complete scan should not fail because of one inaccessible child.
- Tag-read failures are per-file errors and do not invalidate the tree.
- Artwork decoding and sizing use `gdk-pixbuf`; invalid image data produces a clear error and preserves the existing draft cover.

## Error and Empty States

- Before opening a directory: prompt the user to choose a music directory.
- No supported audio files: show an explicit empty result under the selected root.
- Directory scan issue: show a concise error/warning without crashing.
- Tag parsing issue: show the selected filename and parsing error in the editor area.
- Non-image or invalid-image drop: reject it and show a cover-panel error.
- No file selected: editor and cover panel show instructions rather than stale data.

## Validation

- Unit tests for extension filtering, recursive filtered tree construction, stable ordering, and `TagDraft` update/restore semantics.
- Run `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings`.
- Run `cargo test`.
- Manually launch with `cargo run` to verify directory selection, expand/collapse behavior, file selection, editable draft updates, restore, image picker, and cover drag-and-drop.

## Decisions

- Directory scanning is recursive.
- The left panel is a tree, not a flat list.
- Each write or restore creates a versioned backup under the selected root before overwriting the source file.
- `audiotags` determines what tags and metadata can be read or written for each supported format; unavailable values render as unavailable rather than inferred.
