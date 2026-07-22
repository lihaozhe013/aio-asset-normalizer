# Menu Bar -- Feature Design & Implementation

**状态:** 已完成 | **提交:** (not committed)

---

## 设计决策

由于 three-d 0.19 不暴露底层 winit window，且 winit 0.28 原生菜单仅支持构造期平台特定 API（Windows `HMENU`、macOS 默认菜单开关），本项目选择 **egui 内置 MenuBar**，在窗口内渲染菜单栏，跨平台一致。

macOS 系统顶部菜单栏暂不可行，待未来迁移至 eframe 或 three-d 暴露 winit window 后实现。

---

## 菜单结构

```
┌──────────────────────────────────────────────────┐
│ File        Edit        View        Help         │
├──────────────────┬───────────────────────────────┤
│                  │                               │
│  控制面板 (egui)  │  3D Viewport (three-d)        │
│                  │                               │
└──────────────────┴───────────────────────────────┘
```

### File
| 选项 | 行为 |
|---|---|
| Import Files... | rfd 单文件选择对话框，添加到文件列表 |
| Import Folder... | rfd 文件夹选择，扫描目录下所有支持的 3D 文件 |
| Clear File List | 清空文件列表 |
| Quit | 退出程序 (FrameOutput.exit = true) |

### Edit
| 选项 | 行为 |
|---|---|
| Reset All to Defaults | 重置 NormalizationConfig 为默认值 |

### View
| 选项 | 显示规则 | 行为 |
|---|---|---|
| Show Grid | `✓` / 空格 | 切换地面网格渲染 |
| Show Axes | `✓` / 空格 | 切换 RGB 坐标轴渲染 |
| Show Origin | `✓` / 空格 | 切换原点球渲染 |
| Reset Camera | — | 重置 OrbitCamera 到默认视角 (4,3,6) → (0,0.5,0) |

### Help
| 选项 | 行为 |
|---|---|
| About AIO Asset Normalizer | 弹出居中模态窗口，显示版本号和 GitHub 链接 |

---

## 实现要点

### 新增文件
```
src/modules/ui/menu_bar.rs    ← 65 行
```

`MenuAction` 枚举定义所有菜单动作，`render()` 函数渲染 `egui::MenuBar` 并返回触发的动作列表。

### 修改文件

| 文件 | 变更 |
|---|---|
| `src/modules/ui/mod.rs` | 添加 `pub mod menu_bar;` |
| `src/modules/viewport/camera.rs` | 新增 `reset()` 方法 (回正视角到默认位置) |
| `src/modules/viewport/canvas.rs` | 新增 `show_axes` / `show_grid` / `show_origin` 布尔字段 |
| `src/modules/ui/file_list.rs` | `add_path()` 改为 `pub`；新增 `clear()`、`scan_folder()` |
| `src/app.rs` | 集成菜单渲染 + 快捷键检测 + 动作分发 + About 对话框 |
| `src/main.rs` | 条件渲染 axes/grid/origin；退出时返回 `FrameOutput.exit = true` |

### 动作分发流程
```
1. collect_shortcut_actions()   → Vec<MenuAction>    (Ctrl+O/Q/R/G/A)
2. menu_bar::render()           → Vec<MenuAction>    (鼠标点击菜单项)
3. dispatch_action() 遍历所有动作                      (修改各组件状态)
4. render_about_dialog()                              (Window 弹窗)
```

### 键盘快捷键
| 快捷键 | 动作 | 实现 |
|---|---|---|
| Ctrl+O | Import Files | `ctx.input()` 检测 `key_pressed(Key::O)` + `modifiers.ctrl` |
| Ctrl+Shift+O | Import Folder | 同上 + `modifiers.shift` |
| Ctrl+Q | Quit | — |
| Ctrl+R | Reset Camera | — |
| Ctrl+G | Toggle Grid | — |
| Ctrl+A | Toggle Axes | — |

### About 对话框
使用 `egui::Window` 居中 (`Align2::CENTER_CENTER`)，由 `show_about: bool` 控制开关。包含版本号 `v0.1.0` 和 GitHub 超链接 (`ui.hyperlink_to`)。

### 退出机制
- `app.quit_requested: bool` 标志位
- MenuAction::Quit → `quit_requested = true`
- Ctrl+Q → 同上
- main.rs 渲染循环返回 `FrameOutput { exit: true, .. }` 退出

---

## 待改进项

- [ ] macOS 系统原生菜单栏 (需 three-d 暴露 winit window 或迁移 eframe)
- [ ] 快捷键在文本输入框聚焦时不拦截 (当前 Ctrl+A 可能与"全选"冲突)
- [ ] Preferences 对话框 (Edit → Preferences)
- [ ] View → Fullscreen 选项
- [ ] 菜单项可使用 `egui::Key::name()` 动态显示平台正确快捷键名
