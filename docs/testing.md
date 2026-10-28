# 测试说明

常规命令见 [AGENTS.md](../AGENTS.md#验证改动)。前端单元测试在 `src/`，Rust 单元测试与输入模型一起维护；默认 `cargo test` 忽略真实桌面输入测试，不能据此认定原生录放已验证。

## 原生测试入口

在 Windows 交互桌面、仓库的 `src-tauri/` 目录运行。测试使用固定样本与专用 Win32 接收窗口，会切换焦点、移动鼠标并发送键盘输入，应逐项执行，避免同时操作其他应用。独立测试使用 F6 / F7，避免与应用默认 F8 / F9 冲突。

```powershell
cargo test --test native_contract native_capture_edit_save_replay_contract -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_unicode_stop_and_cleanup -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_injected_click_through_stop_region -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_stop_control_preserves_interleaved_key_release -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_edge_move_execution_copy_preserves_raw -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_keys_edit_preservation -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_environment_messages_cancel_before_input -- --ignored --nocapture --test-threads=1
cargo test --test native_edge_diagnostics native_injected_left_edge_hook_coordinates -- --ignored --nocapture --test-threads=1
cargo test native_click_highlight_pixels_and_passthrough -- --ignored --nocapture --test-threads=1
```

这些测试分别覆盖捕获 → 编辑 → 保存重开 → 执行，明确文本与停止信号触发后的收尾，注入点击穿透 Win32 测试窗口与注入停止键过滤，停止点击期间交错输入的保留，键盘编辑保真，环境通知取消，屏幕边缘的钩子与光标位置，以及点位高亮的像素、穿透和消失。穿透、交错输入与边缘测试使用固定的合成输入，不能代替真人界面或物理硬件验收。具体断言以 [native_contract.rs](../src-tauri/tests/native_contract.rs) 为准；[native_roundtrip.rs](../src-tauri/tests/native_roundtrip.rs) 提供更小的钩子和计时器排查用例。

实际 Tauri 状态条验收使用同文件中的 `native_tauri_overlay_injected_click_receiver`：先启动前端开发服务器，将 `REPEAT_E2E_EXECUTABLE` 设为已编译的 Repeat 可执行文件绝对路径，再单独运行该 ignored 测试并按提示在隔离实例中启动任务。设置 `REPEAT_E2E_PHYSICAL_STOP=1` 可改测真人停止及 Ctrl 释放；结束后还原环境变量。测试只使用 `target/` 下的隔离数据，不读取用户任务。

录制状态条验收入口为 `cargo test --test native_recording_overlay native_recording_overlay_stop_preserves_prior_click -- --ignored --nocapture --test-threads=1`，同样需前端开发服务器和 `REPEAT_E2E_EXECUTABLE`；在隔离实例中开始录制，点击一次空白处，再点状态条停止。测试核对先前点击成对保留，停止点击未录入。

长移动与停顿样本使用同一测试，设置时长后运行并还原环境变量：

```powershell
$previousRepeatNativeSeconds = $env:REPEAT_NATIVE_SECONDS
try {
    $env:REPEAT_NATIVE_SECONDS = "180"
    cargo test --test native_contract native_capture_edit_save_replay_contract -- --ignored --nocapture --test-threads=1
} finally {
    $env:REPEAT_NATIVE_SECONDS = $previousRepeatNativeSeconds
}
```

默认约 6 秒是混合键鼠样本；时长 ≥ 20 秒时测试仅保留移动样本，接收器不记录鼠标移动。因此 `180` 秒用例验证长停顿与调度，不能替代三分钟混合操作验收。

以下用例同在 `native_contract.rs`，用上面的命令替换测试名，按所需环境单独运行，不能一次执行全部 ignored 测试：

| 测试名 | 额外条件与影响 |
| --- | --- |
| `native_app_execution_receiver` | `REPEAT_VERIFY_APP_DB` 指向隔离实例的 `tasks.sqlite3`；需在该实例中选择固定测试任务并启动三轮执行 |
| `native_environment_display_change_cancels` | 短暂改变分辨率并自动恢复，屏幕会闪动 |
| `native_environment_lock_cancels` | 真实锁屏，结束后需用户解锁 |
| `native_environment_suspend_resumes_and_cancels` | 真实睡眠约 25 秒；唤醒计时器不可用时需手动唤醒 |
| `native_uipi_restricted_target_receives_nothing` | 需要 UAC 确认启动管理员接收器，与普通权限接收器对照 |

应用联动时用 `REPEAT_DATA_DIR` 启动隔离实例，再将 `REPEAT_VERIFY_APP_DB` 指向其中的数据库；只使用固定测试任务。捕获契约测试检测到此变量也会联动该实例，独立测试时应保持未设置，测试后还原。锁屏、睡眠及 UAC 测试需安排用户可配合的时间。

## 事件证据

核对两层：已知测试输入对原始捕获，当前修订的编译事件对接收窗口。序号连续不能单独证明没有漏捕；目标应用最终文字正确也不能证明按键、顺序和时序完整。

- 保留样本版本、系统与显示器 / DPI、目标程序、负载、时长，以及事件种类、数量、顺序和原有停顿；固定输入标记用于区分样本与桌面其他输入。
- 键盘核对 down/up、重复按下、扫描码和扩展位。Win32 修饰键消息的通用 VK 不足以区分左右键，要看 lParam；IME 生成字符与物理按键分别判断。
- 停止后不得继续注入计划事件，仅允许必要释放；核对本次持有的键鼠均释放且指针不跳回。
- `plannedUs` / `actualUs` 相对当前轮起点；`actualUs` 在注入调用成功返回后采样。比较偏差分位数、最大值和首轮首末误差差值，不与独立接收器时钟直接相减。单次停止观察值不能充当端到端延迟或 P95。

测试自动生成证据到 `src-tauri/target/`：`native-contract-evidence.json`、`keys-edit-evidence.json`、`app-execution-evidence.json`，UIPI 用例生成 `uipi-evidence.json`。状态条穿透的 Win32 替身窗口固定样本写入 `overlay-injection-evidence.json`：停止区域内的注入点击到达底层接收器，注入 Esc 和配置停止键未触发停止回调。实际 Tauri 状态条的隔离样本证据写入 `tauri-overlay-injection-evidence.json`：单屏 1920×1080 / 125% 下，蓝色边框像素可见、边框未取得焦点；停止区域物理坐标 (1112, 973) 的注入点击到达底层接收器，随后收到 F6 down/up，回放继续。`tauri-overlay-physical-stop-evidence.json` 的最近一次样本改为执行期间关闭主窗口：窗口关闭后系统 Ctrl 不再按下，但探针未记录带 Repeat 标记的 Ctrl 抬起事件，需继续核对退出收尾。相同测试重跑会覆盖输出，应将需要留存的固定样本证据附在对应变更或测试归档中，不提交普通用户录制内容。后端还有 `get_execution_report`，当前界面没有报告查看或导出入口。

本机单屏 1920×1080 / 125% 的录制状态条固定样本在 `recording-overlay-evidence.json` 中保留了普通点击的 down/up，停止区域未出现点击事件。`stop-control-interleaved-evidence.json` 记录了通过 Windows 钩子捕获的合成样本：按键抬起发生在停止点击的 down/up 之间，移除停止点击后该按键仍完整配对，其他鼠标移动亦保留；该样本不代表真人界面操作。`left-edge-hook-evidence.json` 记录了向左的固定注入移动：低级钩子报告屏幕左侧外 2 像素时，系统光标仍位于左边缘；它不能证明真实物理鼠标的所有行为。`edge-replay-evidence.json` 记录了边缘越界移动的执行副本映射、实际二维光标位置和接收窗口的按键按下／抬起，原始记录未改变。`click-highlight-evidence.json` 记录了蓝色点位像素、穿透到专用接收器的鼠标 down/up，以及提示关闭后像素恢复。

内部计时参考目标是空载 P95 ≤ 5 ms、P99 ≤ 15 ms，三分钟首末误差差值绝对值 ≤ 20 ms，原生停止观察 P95 ≤ 50 ms；它们用于比较回归，不是用户验收门槛或所有环境的性能承诺。

## 界面回归

在 `pnpm tauri dev` 的实际窗口操作；浏览器模拟仅作辅助。按改动选择相关场景：

- 两个任务分别录制、切换、保存重开；重录放弃保留原任务，保存失败有重试且不假报成功。
- 逐类编辑、取消、撤销 / 重做、删除；插入一秒等待后所有后续动作与总时长增加一秒，删除等待恢复时间，删除录制动作保留后续时间。
- 时间线等待拖动、连续定位、缩放锚点与百分比输入、上缘调高、末尾刻度；列表和预览同步，面板显隐保留当前位置。
- 参考图选点的桌面坐标、侧栏调宽、小窗口可用性；演示进入 / 重置 / 退出保持真实任务隔离。
- 三分钟混合录放覆盖开头 / 中间 / 结尾停顿、左右中键、双击、滚轮、长按、组合键与拖拽；另查三轮执行、无限循环停止、带修饰键的全局停止及收尾。

## 验证边界

平台目标为 Windows 10+ x64；Linux X11 的后续目标环境为 Ubuntu 24.04 LTS / Xorg，Wayland 单独验证。开发和验收状态集中在本节维护。

已有历史证据来自 Windows 10 22H2 x64、本地单屏 1920×1080 / 125% 环境：短混合输入、三分钟移动与停顿、应用三轮执行、明确文本、停止收尾、键盘编辑与真实分辨率切换取消。这些是特定样本的记录，不等于当前版本全量验收；时间线和部分工作区交互也有实际窗口操作记录。时间线 canvas 的文字行起点与 1px 描边已对齐设备像素网格，并经实机窗口（PrintWindow 抓图加像素聚类指标）复核：色块边框锐利、文字位置正确；12px 中灰刻度文字残留的柔和属该字号颜色的栅格化地板而非位置缺陷，徽章文字同路径但尚未在带等待数据的窗口实测。

以下事项尚缺完整验证记录，或仍未实现：

| 事项 | 当前状态与剩余工作 |
| --- | --- |
| Windows 功能回归 | 已实测三分钟混合键鼠录放、全部动作编辑、任务切换与保存重开、演示隔离、应用捕获联动与三轮执行；尚缺参考图选点、侧栏调宽与小窗口、保存失败重试，以及无限循环停止的界面驱动执行 |
| 显示与应用兼容 | 多屏、负坐标、不同 DPI、更多目标应用待验证 |
| 特殊环境与停止 | 单屏 1920×1080 / 125% 下，录制状态条、执行蓝框及注入点击穿透已按固定样本核对。执行期间关闭主窗口可退出，但 Ctrl 抬起的探针证据不完整；目标窗口的 Ctrl 抬起消息及跨 DPI 命中待进一步核对。UIPI 权限受限目标、真实锁屏和睡眠用例已编写，尚待实测 |
| 性能与规模 | 长时间录制、高频输入及繁忙负载下的稳定性、内存和磁盘增长待测量 |
| 打包与安装 | 已构建 NSIS 安装包并验收安装、独立启动与卸载；MSI 暂缺（WiX 工具链无法下载） |
| Linux X11 | 后端未实现；还需输入链路、桌面界面及安装验证 |
| Wayland | 全局捕获、注入、坐标及授权的可行性待独立验证 |
