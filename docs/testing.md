# 测试说明

常规检查见 [AGENTS.md](../AGENTS.md#验证改动)。改界面交互时，用 `pnpm tauri dev` 打开实际窗口验证。改捕获、注入、调度或系统停止时，在目标 Windows 环境运行对应的原生用例并核对事件证据。默认 `cargo test` 跳过这些用例。

## 运行原生测试

在 Windows 交互桌面，从 `src-tauri/` 单独运行所选用例。测试会抢焦点、移动鼠标或发送按键。除测试提示要求操作外，保持键鼠空闲。涉及应用的测试使用固定样本和隔离数据目录。

改动涉及录制、编辑或保存重开时，可以先跑一次短样本，核对原始捕获和接收窗口。

```powershell
cargo test --test native_contract native_capture_edit_save_replay_contract -- --ignored --exact --nocapture --test-threads=1
```

可用 `cargo test -- --ignored --list` 查找测试名。集成用例把 `--test` 后的名字换成对应文件名（不含 `.rs`），库内用例改用 `--lib`，并用列表中的完整测试名过滤。逐个运行，避免一次跑全部 ignored 用例。

| 要验证的改动 | 选用例与准备 |
| --- | --- |
| 录放保真、键盘编辑、停止收尾、执行边缘坐标 | 从 [native_contract.rs](../src-tauri/tests/native_contract.rs) 选用例。要测长停顿与调度，可为上面的短样本设置 `REPEAT_NATIVE_SECONDS=180`。时长达到 20 秒后样本仅包含鼠标移动，接收器也不记录移动，三分钟混合录放需另做实机操作。 |
| 实际 Tauri 状态条、录制准备和停止 | 按表后说明准备开发服务器与可执行文件。执行状态条选 `native_tauri_overlay_injected_click_receiver`，按提示以 1× 单次执行，真人点击停止时设 `REPEAT_E2E_PHYSICAL_STOP=1`。录制准备和停止按用例的终端提示操作。见 [native_contract.rs](../src-tauri/tests/native_contract.rs) 和 [native_recording_overlay.rs](../src-tauri/tests/native_recording_overlay.rs)。 |
| 执行中关闭主窗口的收尾释放 | 用 [native_contract.rs](../src-tauri/tests/native_contract.rs) 的 `native_app_window_close_releases_held_ctrl`。将 `REPEAT_E2E_EXECUTABLE` 指向调试版可执行文件，无需启动开发服务器。用例在回放持有 Ctrl 时向主窗口投递 `WM_CLOSE`，核对接收端抬起消息的扫描码和扩展位、钩子侧的注入配对及进程退出码。测试期间让普通权限窗口保持可获焦点。 |
| 保存失败后界面重试 | 用 [native_save_retry.rs](../src-tauri/tests/native_save_retry.rs) 的 `native_save_failure_ui_retry`。按表后说明准备开发服务器和调试版可执行文件，并确保 `PATH` 中有 `ffprobe`。用例锁住隔离数据库模拟写入失败，核对错误提示和原任务，通过界面点击「重试保存」，再重开核对动作和录像时长。 |
| 已打开的应用与接收器联动 | 用 `REPEAT_DATA_DIR` 启动隔离实例，把 `REPEAT_VERIFY_APP_DB` 指向其中的 `tasks.sqlite3`。在 [native_contract.rs](../src-tauri/tests/native_contract.rs) 选 `native_app_execution_receiver`，等接收器就绪后在隔离实例中运行固定任务三轮。上面的短样本也会读取此变量，独立运行前须清除它。 |
| 桌面录像、像素、捕获排除 | 使用带系统媒体组件的 Windows 10 2004 或更新版本。编码与性能用例见 [screen_capture.rs](../src-tauri/src/platform/windows/screen_capture.rs)。验证实际画面、悬浮窗排除及对应输入时，用 [native_screen_video.rs](../src-tauri/tests/native_screen_video.rs)，需本机 FFmpeg 解码像素。 |
| 显示变化、锁屏、睡眠、权限隔离 | 改到环境监视时，先跑 [native_contract.rs](../src-tauri/tests/native_contract.rs) 的环境消息用例。需要确认系统实际投递通知时，再单独测试真实分辨率切换、锁屏或睡眠。锁屏后需手动解锁，睡眠可能需要手动唤醒。改到注入权限或要验证管理员目标时，单独运行 UIPI 用例，按 UAC 提示启动管理员接收器。 |

需要操作 Repeat 界面的用例会自行启动隔离实例。先从仓库根目录运行 `pnpm dev`，再将 `REPEAT_E2E_EXECUTABLE` 指向已编译的调试版 Repeat 可执行文件绝对路径。

录制停止用例 `native_recording_overlay_stop_preserves_prior_click` 默认测状态条按钮，核对普通点击、末尾移动和停止手势。主窗口按钮、F10 和快速双击分别通过 `REPEAT_E2E_MAIN_STOP=1`、`REPEAT_E2E_HOTKEY_STOP=1`、`REPEAT_E2E_MAIN_RACE=1` 单独测试。快速双击若未命中竞态，记为结果未定。

测试结束后还原临时环境变量。`REPEAT_E2E_PHYSICAL_STOP` 等开关按变量是否存在判断，设为 `0` 仍会启用。

## 界面回归

用 `REPEAT_DATA_DIR` 指向临时目录，再从仓库根目录启动 `pnpm tauri dev`。按改动选择对应场景。

- **录制与任务保存**。在两个任务间切换、录制并重开。取消准备或放弃重录后核对原任务，模拟保存失败时核对错误提示与重试。检查倒计时内的输入和停止按钮点击未录入，结束前的普通操作及末尾停顿保留。
- **编辑与预览**。检查保存、取消、撤销和重做。插入等待后核对动作与时长，拖动、缩放时间线时核对列表与预览同步。改到时间线绘制或交互时，顺带核对窄等待标记能否看见、选中和拖动。改到录像时，检查重开后的播放、定位及编辑时间后的画面对应关系。
- **系统执行与停止**。用混合键鼠样本覆盖开头、中间和结尾的停顿、组合键和拖拽。按改动核对多轮执行、无限循环停止、带修饰键的全局停止及收尾。涉及真实输入时，按下节核对事件证据。
- **演示与工作区**。演示进入、重置和退出后核对真实任务仍可继续使用。涉及选点、缩放或布局时，在实际窗口检查坐标、面板和小窗口操作。

## 事件证据

录制时逐项对照已知输入与原始事件。执行时对照当前修订的编译事件与接收窗口。序号连续和最终文字正确只够做初筛。

- 记录样本版本、Windows 版本、屏幕布局与 DPI、目标应用、负载和时长。对照事件类型、数量、顺序、按下与抬起配对及原有停顿。键盘编辑还应检查重复按下、扫描码与扩展位。左右修饰键应查 Win32 消息的 `lParam`，IME 生成文字与物理按键分开核对。
- 核对停止后只释放本次执行仍持有的输入，后续计划事件未注入，指针未跳回。
- `plannedUs` 和 `actualUs` 均相对当前轮起点，后者在注入调用成功返回后采样。比较误差分位数、最大值和首轮首末误差差值。接收窗口的时间来自独立时钟，单独分析。空载 P95 ≤ 5 ms、P99 ≤ 15 ms、三分钟首末误差差值绝对值 ≤ 20 ms、原生停止观察 P95 ≤ 50 ms 是内部回归参考。分位数需用多次样本，目标应用处理延迟需另测。

原生用例的证据写入 `src-tauri/target/`，如 `native-contract-evidence.json`。具体文件名以测试源码和终端输出为准。重跑可能覆盖文件，需要排查或复现问题时再另存相关证据。只分享固定样本的事件证据，避免提交用户输入和桌面录像。

短样本遇到超时停止时，先检查 `native-contract-failure-evidence.json`。若出现单次注入调用长时间阻塞或鼠标标记缺失，可单独跑 [native_mouse_marker_probe.rs](../src-tauri/tests/native_mouse_marker_probe.rs)，分别对照注入耗时和钩子收到的标记。该探针使用合成输入，物理设备的捕获情况需另测。
