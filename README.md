# Sleeve

一个基于 **Rust + Relm4 + GTK4** 的桌面音频标签与封面编辑器。

## 功能

- 选择音乐目录并递归浏览 MP3、FLAC、M4A/M4B/MP4 文件。
- 树状文件列表，按目录展开/收起。
- 编辑标题、艺人、专辑、专辑艺人、年份、曲目号、碟号和流派。
- 读取并显示容器格式、编码、时长、码率、采样率、声道、位深及文件大小。
- 预览、替换或移除封面；支持拖放图片。
- 保存前自动创建版本化备份。
- 从 HeaderBar 右侧的“恢复备份”菜单还原历史版本。

## 安全保存与恢复

每次保存或恢复前，Sleeve 会先在所选目录中创建隐藏备份：

```text
<音乐目录>/.sleeve-backups/<时间戳>/<原始相对路径>
```

例如：

```text
Music/.sleeve-backups/2026-07-18T14-32-08/Album/01 - Track.flac
```

恢复操作同样会先备份当前文件，因此恢复本身也可以再次撤销。

## 支持格式

标签读写由 `audiotags` 提供，目前支持：

- MP3
- FLAC
- M4A / M4B / MP4

## 运行

需要安装 GTK4 开发环境。

### macOS

```sh
brew install gtk4 libadwaita
cargo run
```

macOS 下 HeaderBar 使用原生窗口控制，并通过原生 `NSWindow` 配置禁用全屏行为，同时保留窗口缩放/最大化能力。

### Linux（Debian/Ubuntu）

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev
cargo run
```

## 开发检查

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
