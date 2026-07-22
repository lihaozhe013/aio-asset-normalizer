# Milestone 1.0 -- MVP 核心管线与 3D 预览

**状态:** 已完成 | **提交:** `cdee792`

---

## 概述

M1 实现了 aio-asset-normalizer 的最小可行产品。用户可通过双栏 GUI 导入 3D 资产文件，配置标准化参数，后台调用 Blender 执行批量转换，并在 3D 视口中预览转换后的 `.glb` 模型。

---

## 架构

```
src/
├── main.rs                        入口，初始化窗口并运行 render loop
├── app.rs                         顶层状态机：Camera, Canvas, FileList, Config, Log
├── modules/
│   ├── ui/
│   │   ├── file_list.rs           文件导入面板 (rfd 文件对话框 + 拖拽放置)
│   │   ├── config_panel.rs        标准化配置 (缩放比、朝向轴、清理项)
│   │   ├── log_viewer.rs          日志查看器 (后台 stdout/stderr 实时输出)
│   │   └── fonts.rs               系统 CJK 字体回退加载
│   ├── viewport/
│   │   ├── camera.rs              OrbitCamera (右键旋转、中键平移、滚轮缩放)
│   │   ├── canvas.rs              视口画布 (坐标轴、网格、原点球、GLB 模型)
│   │   └── helpers.rs             3D 辅助线构建
│   └── blender/
│       ├── bridge.rs              Blender 进程调度 (发现可执行文件、衍生子进程)
│       └── task.rs                任务定义与进度消息结构
blender_scripts/
└── normalize_v1.py                静态模型标准化 Python 脚本
```

---

## 功能清单

### UI 面板
- 可缩放双栏布局 (egui Panel + three-d 视口)
- **文件列表:** `rfd` 原生文件对话框、手动路径输入、拖拽放置、去重过滤、单文件移除/清空
- **标准化配置:** DragValue 缩放比、ComboBox 朝向轴选择、四项清理策略勾选
- **日志查看器:** 等宽滚动文本区、自动滚动/清空、实时输出 Blender 日志
- 中文界面支持 (Windows: 微软雅黑, macOS: PingFang, Linux: Noto Sans CJK)

### 3D 视口
- RGB 彩色坐标轴 (红=X, 绿=Y, 蓝=Z) + 灰色地面网格 + 白色原点球
- OrbitCamera: 右键拖拽旋转、中键拖拽平移、滚轮缩放 (距离限制 0.5~50)
- 视口区域自动适配左侧面板宽度
- 支持加载 `.glb` 模型并用 PBR 材质 + 环境光/方向光渲染

### Blender 桥接
- 自动发现 Blender 可执行文件 (`BLENDER_PATH` 环境变量 > 常见安装路径 > PATH)
- 自动解析 `normalize_v1.py` 脚本路径 (二进制同级目录 > 开发目录)
- 子进程派生: `blender -b -P normalize_v1.py -- <input> <output> <config_json>`
- 非阻塞 stdout/stderr 管道读取，通过 mpsc 通道发送到 UI 日志面板
- 转换完成后自动加载输出的 `.glb` 到 3D 视口

### normalize_v1.py 能力
| 功能 | 说明 |
|---|---|
| 格式导入 | FBX, OBJ, Blend, GLB/GLTF |
| 目标缩放 | 统一应用变换 (Apply Scale) |
| 朝向轴 | Y-Up (export_yup=True) / Z-Up (export_yup=False) |
| 清理策略 | 移除相机、灯光、无用材质、游离顶点 |
| 格式导出 | GLB (PBR 材质、法线、UV、顶点色) |

---

## 数据流

```
用户操作 (添加文件、配置参数)
    │
    ▼
App::start_conversion()
    │  序列化 NormalizationConfig → JSON
    │  创建 mpsc channel
    ▼
std::thread::spawn
    │  遍历文件列表
    ▼
bridge::run_task()
    │  发现 Blender 可执行文件
    │  派生子进程: blender -b -P normalize_v1.py -- ...
    │  stdout/stderr → mpsc::Sender
    ▼
App::poll_tasks() (每帧轮询)
    │  mpsc::Receiver → LogViewer
    │  标记 needs_reload = true
    ▼
App::reload_model_if_needed()
    │  canvas.load_glb() → Model<PhysicalMaterial>
    ▼
main.rs render loop
    ▼
3D 视口渲染模型
```

---

## 依赖

| Crate | 用途 |
|---|---|
| `three-d` (egui-gui) | 3D 渲染引擎 + egui 集成 |
| `three-d-asset` (gltf) | .glb 文件导入 |
| `egui` / `eframe` | GUI 框架 |
| `gltf` (1.4) | GLB/GLTF 解析 |
| `serde` / `serde_json` | 配置序列化 |
| `rfd` (0.15) | 原生文件对话框 |

---

## 已知限制

- 需要安装 Blender (3.6+) 才能执行转换
- 拖拽放置依赖 egui 事件转发 (three-d 集成可能不完全支持)
- PBR 光照使用简单的环境光+方向光，无 IBL/HDR 环境贴图
- 单线程转换 (一次一个文件)，未优化大批量任务
- GLB 加载要求场景名为 "Scene" 或 "scene" (Blender 默认导出)

---

## 下一里程碑

**M2.0 -- 骨骼可视化与动画播放**
- 解析 GLB 骨骼层级并可视化
- 骨骼树侧边栏 UI
- 动画片段播放控制 (播放/暂停/进度/循环/倍速)
- Blender Bridge V2: 蒙皮网格骨骼纠偏与动画烘焙
