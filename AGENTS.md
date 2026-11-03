# AGENTS.md

Repeat 是 Tauri 2 + React/TypeScript + Rust 桌面键鼠录制工具。项目介绍和用法见 [README](README.md)，主要选型、理由和架构边界见[技术架构决策](docs/decisions/technical-selection.md)。目前只有 Windows 输入后端，不能把 Linux 候选方案写成已支持。

## 代码入口

- `src/`：界面、预览、时间线与 `*.test.ts` 测试；`App.tsx` 组织工作区，`api.ts` 对接 Rust 命令。
- `src-tauri/src/`：`session.rs` 管会话，`document.rs` 管动作与编辑，`scheduler.rs` 管调度，`storage.rs` 管保存；`platform/windows/` 管系统输入。
- `src-tauri/tests/`：真实 Windows 输入测试，默认忽略，操作方法见[测试说明](docs/testing.md)。
- `src-tauri/target/` 是生成产物，不手改、不提交。

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

- 涉及真实捕获或注入时，按[测试说明](docs/testing.md)保留事件级证据；模拟测试和目标应用最终显示正确均不能替代原生核对。
- pre-commit 对暂存的脚本文件执行 lint / 格式化，pre-push 运行 `pnpm typecheck`。已通过且输入未变的检查不必重复。
- 仅改文档或注释时，核对实现、链接和失效引用；无需为此启动真实录放或增加测试。

## 文档维护

- 根 README 面向初次了解项目的读者，只放项目介绍、主要功能、上手和开发入口；不放开发进度、验收状态、待办清单或内部实现细则。
- 技术方案或其长期架构边界变更时，更新 `docs/decisions/technical-selection.md` 中的决策与依据。局部实现细节查源码和测试。
- 测试方法、证据入口、验证状态与剩余事项更新 `docs/testing.md`。
- 在原章节直接替换旧结论，不追加“第 X 轮”、对话过程、完成清单或每次测试数量。只有结论、边界或复现方法改变才更新文档。
- 已完成计划、旧原型和一次性输出交给 Git 历史或测试产物，不再维护重复的需求、进度与目录索引。新增文档须有独立查阅用途。
- 代码证明“有实现”，实机证据才证明“在该环境已验证”。未验证能力、候选方案和当前支持范围必须区分。
