# Repeat

桌面键鼠录制、编辑与回放工具：在指定的开始、停止之间连续监听系统键鼠操作，保存动作、轨迹与时间关系，编辑后按原节奏复现。面向 Windows（先行交付）与 Linux X11；界面适配桌面窗口，不做手机端。

> 当前状态：工程脚手架已建立（Tauri 2 + Rust + React + TypeScript），[实施计划](docs/implementation-plan.md) 的 M1（Windows 原生录放验证）尚未开始。本仓库目前不包含任何系统级录制或注入能力。

## 文档

产品需求、技术方案与实施计划位于本仓库 [docs/](docs/README.md)：

| 文档 | 用途 |
| --- | --- |
| [功能需求](docs/requirements.md) | 产品范围、界面行为与验收条件（FR-01～FR-11、AC-01～AC-14） |
| [技术方案 ADR-0001](docs/decisions/0001-technical-selection.md) | 架构职责、平台输入后端、事件模型、调度与存储 |
| [实施计划](docs/implementation-plan.md) | M0～M7 阶段任务、依赖与验收 |
| [交互原型](docs/index.html) | 界面交互基线（画布内模拟，不代表原生能力） |

## 前置条件

- Node ≥ 24 与 pnpm ≥ 12
- Rust stable（经 [rustup](https://rustup.rs/) 安装）
- Windows 构建还需要 MSVC C++ 构建工具与 Windows SDK（[VS Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)；C 盘空间紧张时，安装器支持用 `--installPath` 指定其他盘，本机装在 `D:\soft\BuildTools`），以及 WebView2 运行时（Win10/11 一般自带）
- 在 Git Bash 里跑 cargo 前，需把 MSVC 的 `bin\Hostx64\x64` 加入 PATH，避免命中 Git 自带的同名 `link.exe`（见 [AGENTS.md](AGENTS.md)）
- Linux 的依赖在 X11 交付阶段（M6）另行验证

## 常用命令

| 命令 | 作用 |
| --- | --- |
| `pnpm install` | 安装依赖并注册 git hooks（lefthook） |
| `pnpm tauri dev` | 启动桌面应用（Rust 编译 + 前端热更新） |
| `pnpm tauri build` | 构建发布包 |
| `pnpm lint` / `pnpm format` | oxlint 检查 / oxfmt 格式化 |
| `pnpm typecheck` | TypeScript 类型检查 |
| `pnpm test` | vitest 单元测试 |

Rust 侧检查在 `src-tauri/` 下执行：`cargo fmt --all`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。CI（[ci.yml](.github/workflows/ci.yml)）在推送到 main 和提交 PR 时运行前端检查与 Windows 上的 Rust 检查。

## 目录结构

```
src/          前端：任务、预览、时间线与编辑界面（React）
src-tauri/    Rust：桌面应用与输入引擎（捕获、调度、注入、存储）
docs/         需求、技术方案、实施计划与交互原型
```

两层的职责边界与依赖方向见 [ADR-0001](docs/decisions/0001-technical-selection.md) 第 3 节；输入热路径不允许经过前端。

## 测试

单元测试与源码同目录，按 `*.test.ts` 命名，由 vitest 运行。当前仅有工具链冒烟测试；事件、编辑与调度逻辑的测试随 M1～M3 开发补齐。真实键鼠行为的验证方式（接收端记录、性能测量）见[实施计划第 5 节](docs/implementation-plan.md)。
