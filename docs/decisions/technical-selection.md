# 技术架构决策

以下决策已采用。原生录放与界面回归的测试方法见[测试说明](../testing.md)。

## 用 Tauri 承载界面，Rust 负责系统输入和调度

采用 Tauri 2 和 React/TypeScript，沿用已有的 Web 界面交互。系统级录制需要操作系统输入接口，回放的时序也必须独立于 WebView 刷新。选型时考虑过 Electron 和 Qt。Electron 仍需接入原生输入，Qt 则需要重建已有界面。

捕获、入队、调度、注入和收尾都在 Rust 中完成。界面发送控制命令并接收进度，预览和演示的播放只更新界面。录制与系统执行互斥。见 [commands.rs](../../src-tauri/src/commands.rs) 和 [session.rs](../../src-tauri/src/session.rs)。

## Windows 使用低级钩子捕获，以 SendInput 注入

终点编辑需要桌面坐标。低级鼠标钩子直接提供桌面位置，省去从设备移动量推算指针位置的转换。

Windows 可能静默移除超时钩子，因此进入录制准备、开始捕获或启用执行停止监听时都更新钩子。录制输入经回调过滤后采样入队，缓冲溢出时报告录制失败。物理按键保留扫描码与扩展标志，明确文本使用 Unicode 注入。见 [Windows 输入后端](../../src-tauri/src/platform/windows/)。

## 保留原始录制，编辑派生动作

原始事件用于保留录制证据，编辑作用于派生动作。列表、预览和执行使用同一任务修订，动作编辑通过键鼠配对校验后才可保存。事件采用相对录制起点的整数微秒与序号，保留同一时刻的先后顺序。总时长独立保存，保留最后一个事件后的停顿。

录制控制手势从样本中排除，同时保留普通移动和交错的其他按键释放，避免裁掉与停止无关的输入。见 [document.rs](../../src-tauri/src/document.rs) 和 [capture.rs](../../src-tauri/src/capture.rs)。

## 以录制桌面的物理坐标为准

录制和执行都按桌面物理坐标解释位置。界面选点先经录制几何与 DPI 上下文换算，再由 Rust 校验，避免把 CSS 或图片像素当成系统坐标。执行前按显示器名称和物理矩形比较布局，布局不匹配时拒绝执行。少量可明确映射的外沿移动仅在执行副本中修正，保留原始位置和预览轨迹。见 [geometry.rs](../../src-tauri/src/geometry.rs) 和 [document.rs](../../src-tauri/src/document.rs)。

## 按原生截止时间回放，严重落后即停止

每轮使用单调时钟起点和事件相对时间计算绝对截止时间，避免把注入与等待耗时累加到后续事件。执行速度由 Rust 统一换算事件时间和总时长。超过迟到阈值时停止，避免补发过期输入改变节奏。执行失败时，界面提示轮次和原因。正常完成、取消和失败都只释放本次回放仍持有的输入。

回放按系统当时的焦点发送当前修订的输入，由操作者准备目标窗口。启动时校验显示器布局和停止通道，倒计时结束时检查已按住的键鼠。真人停止信号与回放注入隔离，避免回放触发自身停止。见 [scheduler.rs](../../src-tauri/src/scheduler.rs)、[session.rs](../../src-tauri/src/session.rs) 和 [Windows 执行入口](../../src-tauri/src/platform/windows/mod.rs)。

## 任务由 SQLite 事务保存，录像独立落盘

SQLite 事务把任务文档和修订号一起提交，按任务 ID 与旧修订限制写入。提交成功才替换会话中的有效数据，避免旧编辑覆盖新修订，也让保存失败时能继续使用原任务。

录像作为本地 MP4 保存，任务文档只存引用，进度 IPC 只传状态。先完成录像编码和落盘，再提交引用。数据库写入失败时，原任务保留，待保存录制可在本次会话内重试。见 [storage.rs](../../src-tauri/src/storage.rs) 和 [session.rs](../../src-tauri/src/session.rs)。

## 录像与输入共用时钟，编辑映射回原始画面

桌面画面由独立线程经 GDI 采集、经 Windows Media Foundation 编码，避免采集和编码阻塞输入钩子。画面和键鼠事件共享录制起点，保持预览同步。编辑后通过时间映射定位原始画面，插入等待时冻结画面，调整动作耗时时映射到对应的原始区间。录像读取仅限当前任务引用的文件。见 [screen_capture.rs](../../src-tauri/src/platform/windows/screen_capture.rs)、[media.rs](../../src-tauri/src/media.rs) 和 [screenRecording.ts](../../src/screenRecording.ts)。
