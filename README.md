# Sleeve

一个基于 **Rust + Relm4 + GTK4** 的桌面音频标签与封面编辑器。

## 功能

- 选择音乐目录并递归浏览 MP3、FLAC、M4A/M4B/MP4 文件。
- 树状文件列表，按目录展开/收起。
- 编辑标题、艺人、专辑、专辑艺人、年份、曲目号、碟号和流派。
- 读取并显示容器格式、编码、时长、码率、采样率、声道、位深及文件大小。
- 预览、替换或移除封面；支持拖放图片。
- 编辑停止 500ms 后自动保存有效标签与封面修改。
- 以当前文件为范围的撤销/重做：macOS 使用 `⌘Z` / `⇧⌘Z`，Linux 使用 `Ctrl+Z` / `Ctrl+Shift+Z`。
- macOS 原生菜单栏，包含打开目录、撤销/重做、侧栏切换、About 和退出操作。
- 支持简体中文和英语；默认跟随系统语言环境。

## 语言

Sleeve 支持简体中文（`zh-CN`）和英语（`en`）。应用启动时按以下优先级检测系统语言：`LC_ALL`、`LC_MESSAGES`、`LANG`。以 `zh`、`zh_CN`、`zh-CN` 或 `zh_Hans` 开头的语言环境使用简体中文，其他情况使用英语。

翻译文件位于 `assets/lang/`：

- `assets/lang/zh-CN.json`
- `assets/lang/en.json`

macOS 打包时，语言文件会被复制到 `.app/Contents/Resources/lang/`，应用会优先从该目录加载它们。

修改界面文案或添加翻译键后，可运行以下命令检查两种语言是否完整覆盖：

```sh
python3 scripts/check_i18n.py
```

## 自动保存与撤销历史

Sleeve 会在每次自动保存前为当前音频文件创建临时快照，用于当前应用会话内的撤销/重做：

```text
<音乐目录>/.sleeve-backups/<时间戳>/<原始相对路径>
```

快照只在本次会话中使用；正常退出时，Sleeve 会先保存待写入的修改，再删除整个 `.sleeve-backups` 目录。撤销历史不会跨应用启动保留。

## 支持格式

标签读写由 `audiotags` 提供，目前支持：

- MP3
- FLAC
- M4A / M4B / MP4

## 从源码运行

从源码构建需要安装 GTK4 和 libadwaita 开发环境。

### macOS

```sh
brew install gtk4 libadwaita
cargo run
```

macOS 下 HeaderBar 使用原生窗口控制，并通过原生 `NSWindow` 配置禁用全屏行为，同时保留窗口缩放/最大化能力。

### macOS 打包

需要额外安装 `dylibbundler`：

```sh
brew install dylibbundler
./scripts/bundle-macos.sh --dmg
```

产物会生成在 `dist/`。打包脚本会将 GTK4、libadwaita 及所需运行时资源放入 `.app`，因此使用生成的 `.app` 或 DMG **不需要**预先安装 GTK4 或 libadwaita。

### Linux（Debian/Ubuntu）

```sh
sudo apt install libgtk-4-dev libadwaita-1-dev
cargo run
```

## Alpha 状态

Sleeve 目前处于 Alpha 阶段。重要音频文件请先保留独立备份，并在副本上验证写入结果。

## 许可证

Sleeve 使用 [GNU General Public License v3.0 或更高版本](LICENSE) 发布。

## 开发检查

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
python3 scripts/check_i18n.py
```
