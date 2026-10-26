# Repeat

桌面键鼠录制、编辑与回放工具：在指定的开始、停止之间连续监听系统键鼠操作，保存动作、轨迹与时间关系，编辑后按原节奏复现。面向 Windows 与 Linux X11；界面适配桌面窗口，不做手机端。

## 开发启动

1. `pnpm install` 后运行 `pnpm tauri dev` 启动桌面应用。
2. 在应用中创建任务，开始录制后操作目标应用；F8 启停录制，F9 停止系统执行。

开发进度、已验证范围和剩余任务统一见[实施计划](docs/implementation-plan.md)。原生录放的验证步骤与接收器见[验证材料](docs/verification/measurement.md)。

## 使用方式

- 创建任务，点击“开始录制”后操作目标应用；点击停止或按 F8 结束。新录制成功保存后替换当前内容，“放弃本次录制”保留原任务。
- 单击动作定位，双击编辑；可以修改移动终点、点击、滚轮和键盘输入，或插入等待、明确文本。确认自动保存，取消不改数据；Ctrl+Z / Ctrl+Shift+Z 撤销、重做本次会话的修改。
- 预览只在应用内播放。“执行到系统”按填写次数实际回放，开始前有 3 秒切换目标应用；F9 可停止。快捷键可在帮助菜单调整。
- 参考图由用户手动导入，只供轨迹对照。任务和图片保存在此设备的应用数据目录，具体文件位置见“帮助 → 关于 Repeat → 诊断信息”。

## 文档

产品需求、技术方案与实施计划位于本仓库 [docs/](docs/README.md)：

| 文档                                                            | 用途                                         |
| --------------------------------------------------------------- | -------------------------------------------- |
| [功能需求](docs/requirements.md)                                | 产品范围、界面行为与验收条件                 |
| [技术方案 ADR-0001](docs/decisions/0001-technical-selection.md) | 架构职责、平台输入后端、事件模型、调度与存储 |
| [实施计划](docs/implementation-plan.md)                         | 实施步骤、依赖与验收                         |
| [交互原型](docs/index.html)                                     | 界面交互基线（画布内模拟，不代表原生能力）   |

## 前置条件

- Node ≥ 24 与 pnpm ≥ 12
- Rust stable（经 [rustup](https://rustup.rs/) 安装）
- Windows 构建需要 MSVC C++ 构建工具、Windows SDK（[VS Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)）与 WebView2 运行时。
- 在 Git Bash 里跑 cargo 前，需把 MSVC 的 `bin\Hostx64\x64` 加入 PATH，避免命中 Git 自带的同名 `link.exe`（见 [AGENTS.md](AGENTS.md)）

## 常用命令

| 命令                        | 作用                                   |
| --------------------------- | -------------------------------------- |
| `pnpm install`              | 安装依赖并注册 git hooks（lefthook）   |
| `pnpm tauri dev`            | 启动桌面应用（Rust 编译 + 前端热更新） |
| `pnpm tauri build`          | 构建发布包                             |
| `pnpm lint` / `pnpm format` | oxlint 检查 / oxfmt 格式化             |
| `pnpm typecheck`            | TypeScript 类型检查                    |
| `pnpm test`                 | vitest 单元测试                        |

Rust 侧检查在 `src-tauri/` 下执行：`cargo fmt --all`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。CI（[ci.yml](.github/workflows/ci.yml)）在推送到 main 和提交 PR 时运行前端检查与 Windows 上的 Rust 检查。

## 目录结构

```
src/          前端：任务、预览、时间线与编辑界面（React）
src-tauri/    Rust：桌面应用与输入引擎（捕获、调度、注入、存储）
docs/         需求、技术方案、实施计划与交互原型
```

两层的职责边界与依赖方向见 [ADR-0001](docs/decisions/0001-technical-selection.md) 第 3 节；输入热路径不允许经过前端。

## 测试

前端测试按 `*.test.ts` 命名，由 vitest 运行；Rust 测试由 `cargo test` 运行。真实键鼠行为需要接收端事件记录与原生验证，方法见[实施计划第 5 节](docs/implementation-plan.md)与[测量方案](docs/verification/measurement.md)。
