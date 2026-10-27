# 测试说明

常规命令见 [AGENTS.md](../AGENTS.md#验证改动)。前端单元测试在 `src/`，Rust 单元测试与输入模型一起维护；默认 `cargo test` 忽略真实桌面输入测试，不能据此认定原生录放已验证。

## 原生测试入口

在 Windows 交互桌面、仓库的 `src-tauri/` 目录运行。测试使用固定样本与专用 Win32 接收窗口，会切换焦点、移动鼠标并发送键盘输入，应逐项执行，避免同时操作其他应用。独立测试使用 F6 / F7，避免与应用默认 F8 / F9 冲突。

```powershell
cargo test --test native_contract native_capture_edit_save_replay_contract -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_unicode_global_stop_and_cleanup -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_keys_edit_preservation -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_environment_messages_cancel_before_input -- --ignored --nocapture --test-threads=1
```

这些测试分别覆盖捕获 → 编辑 → 保存重开 → 执行，明确文本与全局停止收尾，键盘编辑保真，以及环境通知取消。具体断言以 [native_contract.rs](../src-tauri/tests/native_contract.rs) 为准；[native_roundtrip.rs](../src-tauri/tests/native_roundtrip.rs) 提供更小的钩子和计时器排查用例。

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

测试自动生成证据到 `src-tauri/target/`：`native-contract-evidence.json`、`keys-edit-evidence.json`、`app-execution-evidence.json`，UIPI 用例生成 `uipi-evidence.json`。相同测试重跑会覆盖输出，应将需要留存的固定样本证据附在对应变更或测试归档中，不提交普通用户录制内容。后端还有 `get_execution_report`，当前界面没有报告查看或导出入口。

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
| 特殊环境与停止 | UIPI 权限受限目标、真实锁屏和睡眠用例已编写，尚待实测 |
| 性能与规模 | 长时间录制、高频输入及繁忙负载下的稳定性、内存和磁盘增长待测量 |
| 打包与安装 | 已构建 NSIS 安装包并验收安装、独立启动与卸载；MSI 暂缺（WiX 工具链无法下载） |
| Linux X11 | 后端未实现；还需输入链路、桌面界面及安装验证 |
| Wayland | 全局捕获、注入、坐标及授权的可行性待独立验证 |
