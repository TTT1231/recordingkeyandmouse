# AGENTS.md

Repeat 是 Tauri 2 + React/TypeScript + Rust 构建的 Windows 桌面键鼠录制工具。项目介绍和用法见 [README](README.md)，主要选型、理由和架构边界见[技术架构决策](docs/decisions/technical-selection.md)。

## 项目目录与入口

```text
recordingkeyandmouse/
├── src/                         # React 界面、编辑与预览；*.test.ts 与源码放在一起
│   └── components/              # 工作区界面组件
├── src-tauri/
│   ├── src/                     # Rust 会话、编辑、调度与存储
│   │   └── platform/windows/    # Windows 捕获、注入与桌面录像
│   └── tests/                   # 真实 Windows 输入测试，默认忽略
└── docs/
    ├── testing.md               # 测试选择、操作方法与判定依据
    └── decisions/technical-selection.md  # 技术选型与架构边界
```

界面从 [App.tsx](src/App.tsx) 和 [api.ts](src/api.ts) 入手，分别组织工作区和对接 Rust 命令。后端按职责进入 [session.rs](src-tauri/src/session.rs)、[document.rs](src-tauri/src/document.rs)、[scheduler.rs](src-tauri/src/scheduler.rs) 和 [storage.rs](src-tauri/src/storage.rs)。

`src-tauri/target/` 是生成产物，不手改、不提交。

## 硬约束

- 捕获、事件队列、调度和注入留在 Rust。前端不得逐事件指挥系统输入，不得用前端定时器驱动系统回放；预览计时仅用于界面。
- 事件时间使用相对录制起点的整数微秒加事件序号，保留顺序和原始停顿，包括结尾停顿。大整数传入 JavaScript 前先换算，避免精度损失。
- 前端 CSS 坐标必须经过录制桌面几何与 DPI 上下文转换，再由 Rust 校验；不能直接送入原生注入。
- 录制与系统执行互斥；预览、演示不得调用注入接口。停止和失败只释放本次回放仍持有的输入。
- 编辑在副本校验，通过后按任务 ID 与修订号保存；写入失败不替换有效数据，不假报成功。动作归并不得丢失原始事件，删除和时间编辑不得破坏键鼠配对。
- 原始键盘数据与录制文本不写入普通日志，不自动上传。

## 验证改动

- 改前端：`pnpm lint`、`pnpm typecheck`、`pnpm test`；界面行为改动需在 `pnpm tauri dev` 中实际操作确认。
- 改 Rust：在 `src-tauri/` 执行 `cargo fmt --all`、`cargo clippy --all-targets -- -D warnings`、`cargo test`。
- 本机在 Git Bash 中执行任何 cargo 命令前，先把 MSVC 链接器目录放到 PATH 最前，避免命中 Git 的同名 `link.exe`（`link: extra operand`）：

    ```sh
    export PATH="/d/soft/BuildTools/VC/Tools/MSVC/<版本号>/bin/Hostx64/x64:$HOME/.cargo/bin:$PATH"
    ```

    版本号用 `ls /d/soft/BuildTools/VC/Tools/MSVC` 查看；cmd / PowerShell 不受此冲突影响。

- 涉及真实捕获或注入时，按[测试说明](docs/testing.md)核对事件级证据；模拟测试和目标应用最终显示正确均不能替代原生核对。
- pre-commit 对暂存的脚本文件执行 lint / 格式化，pre-push 运行 `pnpm typecheck`。已通过且输入未变的检查不必重复。
- 仅改文档或注释时，核对实现、链接和失效引用；无需为此启动真实录放或增加测试。

## 文档维护

- 根 README 说明项目做什么、怎么使用；效果图按需补充，开发细节由 AGENTS 和相关专题文档承接。
- 技术方案或其长期架构边界变更时，更新 `docs/decisions/technical-selection.md` 中的决策与依据。局部实现细节查源码和测试。
- `docs/testing.md` 只维护测试目的、适用条件、操作方法和判定依据；不记录测试结论、验证状态或待办，也不另建测试汇总或变更记录。
- 使用方式、约束或测试方法变化时，直接更新原章节；单次测试结果不触发文档更新。
- 已完成计划、旧原型和一次性输出交给 Git 历史或测试产物，不再维护重复的需求、进度与目录索引。新增文档须有独立查阅用途。
- 文档按当前支持范围编写，不把历史测试结果作为当前功能保证。
