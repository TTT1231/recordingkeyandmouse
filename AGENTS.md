# AGENTS.md

Repeat：桌面键鼠录制、编辑与回放工具。Tauri 2 桌面壳；前端在 `src/`（React），Rust 应用与输入引擎在 `src-tauri/`。产品需求、技术方案与实施计划位于本仓库 [docs/](docs/README.md)。

## 硬约束

以下规则来自 [ADR-0001](docs/decisions/0001-technical-selection.md)，违反会直接破坏产品正确性：

- 输入热路径（捕获、事件队列、调度、注入）留在 Rust。前端不得逐事件指挥系统输入，不得用 `setTimeout`、`requestAnimationFrame` 或前端定时器驱动系统回放。
- 事件时间使用相对录制起点的整数微秒加事件序号；时间换算不得损失事件顺序、不得吞掉原始停顿；大整数传入 JavaScript 前先换算，避免精度损失。
- 前端 CSS 坐标与窗口坐标不直接送入原生注入；坐标必须经过桌面几何与 DPI 上下文转换（ADR 第 5.5 节）。
- 录制与系统执行互斥；预览模式不得调用任何注入接口。
- 原始键盘数据与录制文本不写入普通日志。

## 验证改动

- 改前端：`pnpm lint`、`pnpm typecheck`、`pnpm test`；界面行为改动需在 `pnpm tauri dev` 中实际操作确认。
- 改 Rust（在 `src-tauri/` 下执行）：`cargo fmt --all`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。
- 本机在 Git Bash 中执行任何 cargo 命令前，必须先把 MSVC 链接器目录放到 PATH 最前，否则 rustc 会命中 Git 自带的同名 `link.exe`（报 `link: extra operand`）：

  ```sh
  export PATH="/d/soft/BuildTools/VC/Tools/MSVC/<版本号>/bin/Hostx64/x64:$HOME/.cargo/bin:$PATH"
  ```

  cmd/PowerShell 不受影响。MSVC 版本号以 `ls /d/soft/BuildTools/VC/Tools/MSVC` 为准。
- 涉及真实捕获或注入的改动，模拟测试不能替代原生环境验证；按[实施计划第 5 节](docs/implementation-plan.md)保留事件级证据，不以"目标应用最终显示正确"代替核对。
- pre-commit 自动对暂存文件执行 lint 与格式化；pre-push 运行 `pnpm typecheck`。已通过且输入未变的检查不必重复执行。

## 按任务阅读

| 改动内容 | 先读 |
| --- | --- |
| 输入捕获 / 注入后端 | ADR-0001 第 4 节（Windows 低级钩子与 SendInput、X11 RECORD/XTEST） |
| 时间、调度与回放节奏 | ADR-0001 第 6 节 |
| 事件模型、动作归并与编辑语义 | ADR-0001 第 5 节 |
| 存储、失败处理与会话状态 | ADR-0001 第 7 节 |
| 界面行为是否属于已确认范围 | [功能需求](docs/requirements.md)（以 FR/AC 编号核对） |
| 工作顺序与阶段验收 | [实施计划](docs/implementation-plan.md) |

## 文档维护

行为、接口或平台验证结论发生变化时，同步更新 `docs/` 对应文档：产品规则变化进 requirements.md，技术结论进 ADR-0001，进度与范围进 implementation-plan.md。未完成原生验证的能力不得描述为已实现或已支持；此约定见 [docs/README.md](docs/README.md) 的维护约定。
