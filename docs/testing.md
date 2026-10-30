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
cargo test --test native_mouse_marker_probe native_mouse_marker_probe -- --ignored --nocapture --test-threads=1
cargo test native_click_highlight_pixels_and_passthrough -- --ignored --nocapture --test-threads=1
```

这些测试分别覆盖捕获 → 编辑 → 保存重开 → 执行，明确文本与停止信号触发后的收尾，注入点击穿透 Win32 测试窗口与注入停止键过滤，停止点击期间交错输入的保留，键盘编辑保真，环境通知取消，屏幕边缘的钩子与光标位置，以及点位高亮的像素、穿透和消失。穿透、交错输入与边缘测试使用固定的合成输入，不能代替真人界面或物理硬件验收。具体断言以 [native_contract.rs](../src-tauri/tests/native_contract.rs) 为准；[native_roundtrip.rs](../src-tauri/tests/native_roundtrip.rs) 提供更小的钩子和计时器排查用例。

实际 Tauri 状态条验收使用同文件中的 `native_tauri_overlay_injected_click_receiver`：先启动前端开发服务器，将 `REPEAT_E2E_EXECUTABLE` 设为已编译的 Repeat 可执行文件绝对路径，再单独运行该 ignored 测试并按提示在隔离实例中以默认 1x、单次执行启动任务。启动后等待测试结束，避免切换窗口影响接收器焦点。测试检查准备阶段无边框、执行时蓝框可见，以及固定输入穿透状态条。设置 `REPEAT_E2E_PHYSICAL_STOP=1` 可改测真人停止及 Ctrl 释放；结束后还原环境变量。测试只使用 `target/` 下的隔离数据，不读取用户任务。

设置 `REPEAT_E2E_HOTKEY_CONFLICT=1` 可改测录制快捷键冲突：测试线程占用 F8，隔离实例仍应完成固定输入；测试结束后释放快捷键。该模式记录蓝框像素结果，但不以此作为通过条件；与真人停止和动效观察模式分别运行。证据写入 `target/tauri-hotkey-conflict-evidence.json`。

需要延长执行状态条的观察时间时，可设置 `REPEAT_E2E_OBSERVE_SECONDS=15`，让固定任务在执行状态多停留一段时间，检查状态文字、计时与布局；观察完成并等测试结束后还原变量。该选项不能与真人停止模式同时使用。

录制停止验收入口为 `cargo test --test native_recording_overlay native_recording_overlay_stop_preserves_prior_click -- --ignored --nocapture --test-threads=1`，同样需前端开发服务器和 `REPEAT_E2E_EXECUTABLE`；在隔离实例中开始录制，点击一次空白处，再移动到状态条并点击停止。测试核对先前点击成对保留、停止点击未录入、原始轨迹及回放中的末尾移动均保留。设置 `REPEAT_E2E_MAIN_STOP=1` 改测主窗口按钮；设置 `REPEAT_E2E_HOTKEY_STOP=1` 改测快捷键停止：点击空白处后移动鼠标，再按 F10；测试同样核对末尾移动保留。不要同时设置两个变量；结束后还原环境变量。

录制准备的固定样本入口为 `cargo test --test native_recording_overlay native_recording_preparation_excludes_early_input -- --ignored --nocapture --test-threads=1`，同样需要前端开发服务器和 `REPEAT_E2E_EXECUTABLE`。测试自动启动隔离实例，以 F10 开始和取消准备，再完成一次录制：准备期间发送 F6，正式录制后发送 F7，核对取消保留原任务、准备至少 3 秒、仅 F7 成对保存及结尾停顿；准备与录制阶段另核对状态条窗口和客户区尺寸完全一致，不能存在系统边框偏移。证据写入 `target/recording-preparation-evidence.json`。输入服务层另有 `cargo test --test native_recording_preparation native_preparation_has_no_capture_and_cancel_is_scoped -- --ignored --nocapture --test-threads=1`，核对准备期原生回调无缓冲写入、已取消准备不能启动及旧取消信号不影响新准备，证据为 `target/recording-preparation-service-evidence.json`。两者都是合成输入样本，不替代真人停止按钮及快捷键验证。

桌面视频编码用 `cargo test native_screen_capture_creates_local_mp4 -- --ignored --nocapture --test-threads=1` 核对。像素与捕获排除用 `cargo test --test native_screen_video native_video_records_changes_and_excludes_overlay -- --ignored --nocapture --test-threads=1`：测试创建带上红、下蓝色块的黑色窗口及被排除的白色悬浮窗，以固定鼠标点击激活目标、F6 改成白色，同时核对原生捕获事件与目标窗口接收到的 down/up，再用 FFmpeg 解码固定位置的像素，检查变化、颜色、上下方向及捕获排除。此测试需要本机 FFmpeg，仅为测试探针，产品录制不依赖它。视频与证据分别放在 `target/screen-video-*/`、`target/screen-pixels-*/`，不提交桌面录像。

监听失效回归使用 `cargo test native_capture_recovers_removed_idle_hooks -- --ignored --nocapture --test-threads=1`：在输入服务线程移除钩子但保留缓存句柄，模拟系统静默移除后再次开始录制，检查 F6 成对事件及固定鼠标移动，证据为 `target/removed-hooks-evidence.json`。性能探针为 `cargo test native_screen_capture_performance -- --ignored --nocapture --test-threads=1`，每次录制约 5 秒并输出 `target/screen-performance-*/performance.json`：包括实际采样数量、采集与写入时间、最大帧间隔、工作线程与整个测试进程的 CPU 时间、文件大小。CPU 时间除以墙钟时间表示占用一个逻辑核的比例，不能直接当成任务管理器整机百分比；原生颜色转换与异步编码的成本分布在写入调用及媒体工作线程，不再由旧的逐像素转换计数衡量。

主窗口停止竞态可设置 `REPEAT_E2E_MAIN_RACE=1` 运行同一测试：真人先点击主窗口空白处，再快速双击停止按钮的同一位置。独立探针只记录隔离窗口内的物理左键事件并与原始录制对照，写入 `target/recording-main-race-evidence.json`；若第二次点击未完整进入录制则标为未命中，不能视为竞态通过。结束后还原变量。

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

测试自动生成证据到 `src-tauri/target/`：`native-contract-evidence.json`、`keys-edit-evidence.json`、`app-execution-evidence.json`，UIPI 用例生成 `uipi-evidence.json`。状态条穿透的 Win32 替身窗口固定样本写入 `overlay-injection-evidence.json`：停止区域内的注入点击到达底层接收器，注入 Esc 和配置停止键未触发停止回调。实际 Tauri 状态条的隔离样本证据写入 `tauri-overlay-injection-evidence.json`：单屏 1920×1080 / 125% 下，准备阶段最初约 0.8 秒未显示边框，执行时蓝色边框像素可见且未取得焦点；停止区域物理坐标 (1112, 973) 的注入点击到达底层接收器，随后收到 F6 down/up，回放继续。`tauri-overlay-physical-stop-evidence.json` 的最近一次样本改为执行期间关闭主窗口：窗口关闭后系统 Ctrl 不再按下，但探针未记录带 Repeat 标记的 Ctrl 抬起事件，需继续核对退出收尾。相同测试重跑会覆盖输出，应将需要留存的固定样本证据附在对应变更或测试归档中，不提交普通用户录制内容。后端还有 `get_execution_report`，当前界面没有报告查看或导出入口。

本机单屏 1920×1080 / 125% 的录制停止真人操作样本分别写入 `recording-overlay-evidence.json`、`recording-main-stop-evidence.json` 和 `recording-hotkey-evidence.json`：三种停止方式均保留末尾移动，鼠标停止按钮的点击不参与录制。主窗口快速双击样本中，普通点击保留，停止位置点击未录入，回放输入配对完整；第二次点击未完整进入录制，证据写入 `recording-main-race-evidence.json`。`tauri-hotkey-conflict-evidence.json` 的固定样本中，F8 被测试线程占用时，隔离实例执行并向接收器送达完整鼠标及 F6 按下／抬起；用户另行反馈冲突状态下真人停止实测通过，未提供事件级证据。`stop-control-interleaved-evidence.json` 记录了通过 Windows 钩子捕获的合成样本：按键抬起发生在停止点击的 down/up 之间，移除停止点击后该按键仍完整配对，其他鼠标移动亦保留；该样本不代表真人界面操作。`left-edge-hook-evidence.json` 记录了向左的固定注入移动：低级钩子报告屏幕左侧外 2 像素时，系统光标仍位于左边缘；它不能证明真实物理鼠标的所有行为。`edge-replay-evidence.json` 记录了边缘越界移动的执行副本映射、实际二维光标位置和接收窗口的按键按下／抬起，原始记录未改变。`click-highlight-evidence.json` 记录了蓝色圆环像素、透明中心及角落、穿透到专用接收器的鼠标 down/up，以及提示关闭后像素恢复。

同环境下的桌面录像固定样本已验证：准备至少 3 秒后开始，准备期 F6 未写入、正式录制后的 F7 按下／抬起完整保留，取消准备不改旧文档，视频引用保存重开一致。像素样本中，排除白色悬浮窗后底层 RGB 为 `[0,0,0]`，F6 改变底层画面后为 `[255,255,255]`，上方红色与下方蓝色色块通过解码核对，目标收到的点击与 F6 均与原始录制配对。监听失效故障注入在修复前得到 0 个按键事件，重新安装后得到完整 F6 与鼠标样本；这是固定故障的回归证据，不能据此确定历史录制遗漏的唯一原因，旧录像也不能还原当时未保存的事件。

debug 五秒性能样本中，旧路径 5.129 秒采样 26 帧（约 5.07 fps），逐像素颜色转换累计 2.628 秒、单帧约 101 ms，最大帧间隔约 206 ms。改用原生颜色转换、复用缓冲与固定截止时间后，30 fps 设置下 5.053 秒采样 150 帧（约 29.68 fps），最大帧间隔约 53 ms，采集线程 CPU 约 2 秒。另一次样本为约 28.26 fps，整个测试进程 CPU 5.422 秒 / 墙钟 5.257 秒（约 1.03 个逻辑核），其中采集线程约 2.219 秒，文件约 3.73 MB / 5.06 秒；媒体转换与编码工作线程仍有成本。这些是本机桌面短样本，不能外推游戏繁忙负载、所有机器或发布构建。对应证据为 `target/screen-performance-18e359f592a441a0-1/performance.json`、`target/screen-performance-18e35a8bfe6ac454-1/performance.json` 与 `target/screen-performance-18e35b583ca40448-1/performance.json`。

`pnpm tauri dev` 的隔离实例使用数据库备份验证原有约 16 MB 的录像：能读取 1920×1080 历史桌面画面，连续播放抽查无定位请求、无等待事件、无解码丢帧，证据为 `target/video-ui-playback-evidence.json`。新录制约 29.49 秒，热键停止后无需切换任务即可看到 3 个动作、5 个原始事件与可播放画面；固定点击和 F6 的 down/up 完整，解码核对 F6 前黑色、之后白色。证据为 `target/video-ui-recording-evidence.json`，实际界面截图为 `target/video-ui-target-operation.png`。所有测试均操作隔离副本，原有用户任务未被改写；系统窗口截图不可用时通过测试实例的 WebView 调试连接获取实际界面，非浏览器模拟。

状态条在同环境的隔离 `pnpm tauri dev` 实例中，已点击开始系统执行、开始录制并检查准备和正式录制阶段。浅色状态条的实际 WebView 截图为 `target/ui-cleanup-status-execution.png`、`target/ui-cleanup-status-recording.png`：375×65 物理像素（300×52 CSS 像素 / 125%），四角 alpha 均为 0，胶囊内部采样为不透明白色；状态文字无扫光，停止区域尺寸保留，见 `target/ui-cleanup-evidence.json`。原生准备与录制样本中的窗口和客户区均为 375×65，偏移均为 0，排除了额外矩形非客户区；桌面抓图接口未获工具授权，WebView 截图本身不能证明整个系统合成层，边框结论由上述原生几何核对补充。圆环实机样本的 `click-highlight-evidence.json` 包含 192 个边缘混合像素、透明中心与角落，穿透的鼠标 down/up 各 1 个，停止后像素恢复；截图为 `target/click-highlight-after.png`。测试接收窗口独立处理消息，避免同线程等待输入激活造成探针自身阻塞。

内部计时参考目标是空载 P95 ≤ 5 ms、P99 ≤ 15 ms，三分钟首末误差差值绝对值 ≤ 20 ms，原生停止观察 P95 ≤ 50 ms；它们用于比较回归，不是用户验收门槛或所有环境的性能承诺。

本机短混合样本曾在鼠标移动的单次输入调用附近出现约 1 秒延迟，触发调度器超时停止；固定样本失败证据写入 `target/native-contract-failure-evidence.json`，不覆盖先前的通过证据。复测时保留测试监听的一次通过，回放前关闭测试自身监听的一次仍出现约 1 秒延迟，后者记录在 `target/native-contract-no-test-hooks-failure-evidence.json`。关闭运行中的 Repeat 后重测仍出现约一秒延迟；另一轮固定输入校验发现一个鼠标移动时间标记未出现在记录中，证据为 `target/native-contract-capture-failure-evidence.json`，不能据此判断物理鼠标是否漏录。延迟和标记缺失的原因均尚未确定。

`native-mouse-marker-probe-evidence.json` 是仅包含测试专属标记的鼠标对照：两组各 1000 次固定输入的编号及时间均在独立钩子中观察到；其中系统调用仍出现约 0.62–0.63 秒的慢样本，测试线程 CPU 时间低于计时精度。慢点分别发生在测试钩子进入前及退出后；这些时间不能确定系统等待、调度或其他钩子的责任。该结果不证明此前缺失标记已修复，也不能代替物理鼠标验证。

## 界面回归

在 `pnpm tauri dev` 的实际窗口操作；浏览器模拟仅作辅助。按改动选择相关场景：

- 任务图标统一为文件夹，空任务显示 `0 秒`，工具栏显示“录制”和“执行”；录制确认只保留一行准备或替换说明，预览不显示原画面说明栏。
- 两个任务分别录制、切换、保存重开；重录放弃保留原任务，保存失败有重试且不假报成功。
- 录制前显示完整 3 秒准备倒计时，主窗口收起并保留原任务；取消按钮、录制热键及 Esc 可取消，任务修订不变。倒计时结束有键鼠按住时明确拒绝开始；松开重试能正常录制。
- 桌面录像保存重开后可播放与定位，插入等待时冻结画面，移动耗时变化后映射正确，普通动作删除不重写历史录像；录像缺失时显示可重试的错误。
- 逐类编辑、取消、撤销 / 重做、删除；插入一秒等待后所有后续动作与总时长增加一秒，删除等待恢复时间，删除录制动作保留后续时间。
- 时间线等待拖动、连续定位、缩放锚点与百分比输入、上缘调高、末尾刻度；列表和预览同步，面板显隐保留当前位置。
- 参考图选点的桌面坐标、侧栏调宽、小窗口可用性；演示进入 / 重置 / 退出保持真实任务隔离。
- 三分钟混合录放覆盖开头 / 中间 / 结尾停顿、左右中键、双击、滚轮、长按、组合键与拖拽；另查三轮执行、无限循环停止、带修饰键的全局停止及收尾。

隔离实例已核对上述任务图标、时长、按钮、紧凑录制确认与说明栏移除，截图见 `target/ui-cleanup-empty-task.png`、`target/ui-cleanup-record-dialog.png`。演示的移动采样约每秒 10–20 次，显示插值后两段移动的不同指针位置由各约 12 次增加到约 100、190 次；回放仍到达 6.8 秒且表单完成提交。两次样本的 rAF 间隔 P95 均约 6.2 ms，此改动改善稀疏位置造成的跳动，不代表浏览器帧率提升。演示中途重新回放、练习表单重置及退出恢复任务亦已操作核对，汇总为 `target/ui-cleanup-evidence.json`；此短样本不代表所有机器的性能。

## 验证边界

平台目标为 Windows 10+ x64，桌面录像与悬浮条捕获排除要求 Windows 10 2004+ 及系统媒体组件；Linux X11 的后续目标环境为 Ubuntu 24.04 LTS / Xorg，Wayland 单独验证。开发和验收状态集中在本节维护。

已有历史证据来自 Windows 10 22H2 x64、本地单屏 1920×1080 / 125% 环境：短混合输入、三分钟移动与停顿、应用三轮执行、明确文本、停止收尾、键盘编辑与真实分辨率切换取消。这些是特定样本的记录，不等于当前版本全量验收；时间线和部分工作区交互也有实际窗口操作记录。时间线使用设备像素网格对齐文字和细线；旧样式曾在实机窗口通过抓图核对，新样式的文字和边框已由操作者在演示窗口目视确认，窄等待标记尚待实机核对。

以下事项尚缺完整验证记录，或仍未实现：

| 事项 | 当前状态与剩余工作 |
| --- | --- |
| Windows 功能回归 | 已实测三分钟混合键鼠录放、全部动作编辑、任务切换与保存重开、演示隔离、应用捕获联动与三轮执行；尚缺参考图选点、侧栏调宽与小窗口、保存失败重试，以及无限循环停止的界面驱动执行 |
| 桌面录像 | 单屏 1920×1080 / 125% 下，原生 MP4 编码、键鼠引起的画面变化、颜色方向、捕获排除、准备期不录输入、监听失效恢复及视频引用重开已有固定样本；实际 Tauri 窗口已操作开始、停止、取消准备、播放和动作定位，新录像与事件在热键停止后立即显示。旧的大文件录像亦可正常加载。多屏、不同 DPI、音频（未实现）、发布构建帧率与长录制稳定性仍待验证 |
| 显示与应用兼容 | 多屏、负坐标、不同 DPI、更多目标应用待验证 |
| 特殊环境与停止 | 单屏 1920×1080 / 125% 下，录制状态条、执行蓝框及注入点击穿透已按固定样本核对。执行期间关闭主窗口可退出，但 Ctrl 抬起的探针证据不完整；目标窗口的 Ctrl 抬起消息及跨 DPI 命中待进一步核对。UIPI 权限受限目标、真实锁屏和睡眠用例已编写，尚待实测 |
| 性能与规模 | debug 桌面短样本采样约 29.68 fps，已有采集、编码调用、线程与进程 CPU 的聚合探针；长时间录制、高频输入、游戏繁忙负载下的稳定性、内存和长期磁盘增长仍待测量 |
| 打包与安装 | 已构建 NSIS 安装包并验收安装、独立启动与卸载；MSI 暂缺（WiX 工具链无法下载） |
| Linux X11 | 后端未实现；还需输入链路、桌面界面及安装验证 |
| Wayland | 全局捕获、注入、坐标及授权的可行性待独立验证 |
