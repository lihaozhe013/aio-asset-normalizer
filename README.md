# All-in-One Asset Normalizer (3D)

## 项目简介 (Project Overview)

**3D Asset Normalizer** 是一款极轻量、跨平台、高性能的开源 3D 资产批处理与标准化桌面工具。

项目核心解决开源 3D 资源（FBX, Blend, OBJ 等）在进入 Godot，Unity 等现代游戏引擎时常见的**缩放失控、坐标轴错乱（Y-Up/Z-Up）、骨骼方向偏移及材质丢失**等问题。

### 核心设计哲学

1. **轻量与解耦 (Lightweight & Decoupled)：** UI 采用 Rust 原生 GPU 加载（`egui` + `wgpu`），软件体积控制在 $15\text{MB}$ 以内，启动毫秒级，零外部运行库依赖。
2. **职责分离 (Separation of Concerns)：**
   - **Rust App：** 专职负责 UI 交互、配置文件生成、进程调度与标准化 `.glb` 的轻量级 3D 轴向预览。
   - **Blender CLI：** 专职负责在后台执行复杂的网格变换、骨骼重定向、材质修复与格式转换。

## 技术栈选型 (Tech Stack)

| **模块**       | **选型**                                          | **选用理由**                                                                   |
| ------------ | ----------------------------------------------- | -------------------------------------------------------------------------- |
| **编程语言**     | **Rust**                                        | 无 Cgo/CMake 构建噩梦，内存安全，编译产物为单个静态二进制文件。                                      |
| **GUI 框架**   | **`egui`**                                      | 即时模式 UI，官方维护，跨平台极佳，原生支持嵌入 `wgpu`。                                          |
| **3D 渲染引擎**  | **`three-d`** (基于 `wgpu`)                       | Rust 社区 API 极度接近 Three.js 的 3D 库，原生支持加载 `.glb`、网格底板与 3D 轴向渲染。              |
| **底层图形 API** | **`wgpu`**                                      | 自动映射 macOS (Metal) / Windows (Vulkan/DX12) / Linux (Vulkan)，无旧代 OpenGL 包袱。 |
| **进程与异步调度**  | **`std::process::Command` + `std::sync::mpsc`** | 通过消息传递机制实现 UI 线程与后台转换任务的解耦，防止界面卡死。                                         |
| **资产转换引擎**   | **Blender (Headless / CLI)**                    | 行业标准三维数据处理引擎，通过静默后台模式（`-b -P`）运行 Python 自动化处理脚本。                           |

## 系统架构设计 (System Architecture)

```
+-------------------------------------------------------------------+
|                        Rust 应用程序 GUI                          |
|                                                                   |
|  +---------------------------+   +-----------------------------+  |
|  |     2D 控制面板 (egui)    |   |     3D 预览视口 (three-d)   |  |
|  | - 资产导入与批处理列表     |   | - 绘制 RGB 3D 坐标轴 (XYZ)  |  |
|  | - 目标配置 (Scale/Up-Axis)|   | - 绘制地面网格 (Grid Floor) |  |
|  | - 转换日志与进度展示      |   | - 渲染转换后的标准化 .glb   |  |
|  +-------------+-------------+   +--------------^--------------+  |
+----------------|--------------------------------|-----------------+
                 |                                |
                 | (1. 触发任务 / 导出 config)       | (3. 载入 .glb 校验)
                 v                                |
+-------------------------------------------------+-----------------+
|                       后台工作线程 (Worker)                        |
|                                                                   |
|  - 组装命令行命令                                                  |
|  - 调用: blender -b -P normalize_v1.py -- <input> <output> <json> |
+-------------------------------------------------------------------+
```

## 迭代路线图 (Roadmap)

### Milestone 1.0 — MVP 核心管线与 3D 预览 (Minimal Viable Product)

**目标：** 实现静态 3D 模型（FBX / Blend / OBJ）的批量自动标准化转换，并提供具备 3D 坐标轴与网格的交互预览视口。

#### 功能需求 (Functional Requirements)

1. **基础 UI 布局：**
   - 双栏设计：左侧为文件处理列表与配置项，右侧为 3D Canvas 视口。
   - 支持通过系统文件选择框或**拖拽 (Drag & Drop)** 将模型文件/文件夹投入处理列表。
   - 日志面板：实时输出 Blender 后台转换的 stdout / stderr 信息。
2. **转换配置面板 (Normalization Config)：**
   - **目标单位/缩放比：** 统一应用变换 (Apply Scale，如 $1.0\text{ unit} = 1.0\text{m}$)。
   - **目标朝向 (Up Axis)：** 强制统一转换为 **Y-Up, Z-Forward**（ Godot 标准）。
   - **清理策略：** 勾选是否清除无用材质、相机、灯光及游离顶点。
3. **后台转换工作流 (Blender Bridge V1)：**
   - 基于 Python 编写内置的 `normalize_v1.py` Blender 脚本。
   - Rust 接收转换任务后，阻塞/异步调用 Blender CLI，生成清洗好的 `.glb` 存储至缓存目录/目标目录。
4. **3D Canvas 交互视口：**
   - 渲染基于 RGB 的 3D 轴线（红=X, 绿=Y, 蓝=Z）。
   - 渲染空间地面网格（Grid Floor），直观判断模型是否“脚踩实地”或“下沉”。
   - 支持 **Orbit Camera**（鼠标右键旋转、中键平移、滚轮缩放）。
   - **同步加载：** 转换完成后自动在视口中加载导出的 `.glb` 模型供用户肉眼校验。

### Milestone 2.0 — 骨骼可视化与简单动画播放 (Bone & Motion Support)

**目标：** 引入 3D 骨骼层次结构可视化，支持基础动画序列播放，为后续 BVH 动捕重定向奠定数据与 UI 基础。

#### 功能需求 (Functional Requirements)

1. **骨骼层次可视化 (Skeleton Visualization)：**
   - 在 3D Canvas 中，支持开关 **"Show Bones" (显示骨骼)** 选项。
   - 将带有 Armature/Skin 的模型骨骼链绘制为三维线段（Bone Sticks）或锥体，直观校验骨骼 Bind Pose 姿态是否错位。
2. **骨骼树视口组件 (Bone Tree Inspection)：**
   - 左侧新增 **Bone Tree 侧边栏**，递归解析并展示当前模型的骨骼节点树（如 `Root -> Hips -> Spine -> ...`）。
   - 点击 UI 上的骨骼节点，在 3D 视口中高亮对应位置的骨骼。
3. **基础动画预览 (Animation Player)：**
   - 支持解析 `.glb` 内部嵌入的 Animation Clips（动画片段）。
   - 3D Canvas 底部新增极简**动画播放控制条**：播放/暂停、进度条拖拽、循环切换、播放速率调节（0.5x, 1.0x, 2.0x）。
4. **Blender Bridge V2 扩展：**
   - 扩充 `normalize_v2.py` 脚本：新增对带权重的网格 (Skinned Mesh) 的骨骼轴向自动纠偏（自动处理 FBX 导出时常见的尾骨/Leaf Bones 丢失与轴向翻转问题）。

## 项目目录结构规划 (Project Directory Structure)

```
asset-normalizer/
├── Cargo.toml                  # Rust 依赖声明
├── build.rs                    # 编译期资源打包 (如将 Python 脚本打包进二进制)
├── src/
│   ├── main.rs                 # 程序入口，初始化 eframe 窗口
│   ├── app.rs                  # egui 主状态机与 UI 布局分发
│   ├── modules/
│   │   ├── ui/                 # 2D 界面面板组件
│   │   │   ├── file_list.rs    # 文件导入与列表组件
│   │   │   ├── config_panel.rs # 缩放/轴向等配置面板
│   │   │   └── log_viewer.rs   # 后台日志输出组件
│   │   ├── viewport/           # 3D Canvas 渲染模块
│   │   │   ├── canvas.rs       # three-d 视口封装
│   │   │   ├── camera.rs       # Orbit 相机控制器
│   │   │   └── helpers.rs      # 坐标轴与网格绘制逻辑
│   │   └── blender/            # 后台进程调度模块
│   │       ├── bridge.rs       # Command 进程调用与生命周期管理
│   │       └── task.rs         # 线程安全通道 (mpsc) 任务定义
├── blender_scripts/            # 内置的 Blender Python 脚本
│   ├── normalize_v1.py         # V1 静态模型网格/材质标准化
│   └── normalize_v2.py         # V2 骨骼修复与动画烘焙扩展
└── README.md                   # 本文档
```
