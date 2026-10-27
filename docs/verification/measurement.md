# 原生录放测量方法与接收器

定义日期：2026-10-26。对应实施计划 M0-03、M1 与 M5。这里只维护方法、字段和复现入口；执行状态、实际数值与未验收项统一见[实施计划第 8 节](../implementation-plan.md#8-实施与验证记录)。

所有引擎时间以单调时钟（Windows QPC，Rust `Instant`）为基准，日期仅用于标记记录。只采集核对所需证据，不把用户键盘内容写入普通日志。

## 引擎侧数据

### 录制与当前修订

原始录制保存在本地 SQLite 任务文档中，原生测试通过存储接口读取并核对。首版没有“导出核对数据 recording.json”或正式录制导入/导出入口；不要把交互原型的 JSON 当成产品格式。

| 字段           | 含义                                                                   |
| -------------- | ---------------------------------------------------------------------- |
| `seq`          | 按回调到达顺序分配的原始事件序号，与相对时间共同确定顺序               |
| `timeUs`       | 相对录制起点的整数微秒                                                 |
| `nativeTimeMs` | 钩子提供的系统事件毫秒时间，仅作诊断与固定测试样本标记，不作为回放时钟 |
| 事件内容       | 按下/抬起、虚拟键码、扫描码、扩展标志、物理坐标、滚轮轴与原生增量      |
| `durationUs`   | 独立录制起止时长，包含结尾停顿                                         |
| `geometry`     | 录制桌面的物理像素几何、原点与 DPI 上下文                              |
| 动作来源与修订 | 原始事件引用、当前动作和递增修订号；编辑后按当前修订编译执行           |

核对分两层：原始事件与已知测试输入比较，证明捕获完整；当前编译结果与接收消息比较，证明编辑后执行符合预期。不能直接拿编辑后的序列与原始录制逐项相等作判断。

序号连续只能说明应用内部编号连续，不能单独证明操作系统输入没有漏捕。固定样本还需核对标记、事件种类、数量和顺序；测试使用唯一原生时间标记区分样本输入与其他桌面输入。

### 执行报告

应用任务工具栏的“更多 → 上次执行报告”提供查看与 JSON 导出。报告由 Rust 生成，导出的是计时与执行结果，不是可重新导入执行的录制文件。

| 字段       | 含义                                                                                                        |
| ---------- | ----------------------------------------------------------------------------------------------------------- |
| 逐事件计时 | `round`、`seq`、`plannedUs`、`actualUs`；计划与实际均相对当前轮起点，跨轮核对需结合 `round`，不包含轮次偏移 |
| `actualUs` | 原生注入调用成功返回后采样，包含调用耗时；不是调用开始时刻，也不是目标应用处理完成时刻                      |
| 偏差统计   | `actualUs - plannedUs` 绝对值的 P50/P95/P99/最大值                                                          |
| 累计漂移   | 首轮末次调度误差减首次调度误差；不等同于目标应用的业务耗时                                                  |
| 停止观察   | 停止信号发出至调度线程观察取消的时间；还需核对取消后无新的计划输入，仅允许必要收尾释放                      |
| 执行结果   | 请求/完成轮数、成功注入事件数、停止或失败原因和轮次、本次仍按住输入的收尾结果                               |

引擎时钟和接收窗口时钟分别有起点。对齐事件后比较顺序与时间间隔，不直接相减两个独立起点的时间戳。停止观察的单次结果不能当作 P95，也不包含用户按键到系统钩子收到按键的全部延迟。

## 接收侧

### 原生窗口

`src-tauri/tests/native_contract.rs` 包含专用 Win32 接收窗口与 EDIT 控件：

- 短混合样本按消息顺序核对键盘 down/up、鼠标左右中键、双击、垂直/水平滚轮等。物理按键与 IME 生成字符分别判断。
- 键盘保真以 lParam 为准：接收器记录原始 lParam，键盘消息位 16-23 为扫描码、位 24 为扩展键标志。修饰键的 WM_KEYDOWN wParam 是通用 VK（真实硬件同样如此），L/R 身份由扫描码+扩展位表达。
- 明确文本样本在 EDIT 控件核对 UTF-16、中文、emoji、符号和换行。
- 长移动样本检查固定样本事件的完整捕获、原有停顿与逐事件调度时间；该接收器不记录鼠标移动消息，因此接收消息为零不表示未执行，也不能算作混合输入验收。
- 应用三轮执行测试读取隔离实例的任务，并等待从实际界面启动执行，再核对轮次间隔与各类消息数量。
- 环境取消测试分两层：消息路径仅向监视窗口投递显示、挂起、会话锁定消息；真实环境测试实际改分辨率（`native_environment_display_change_cancels`，自动恢复）、锁屏（`native_environment_lock_cancels`，测试后需解锁）与睡眠（`native_environment_suspend_resumes_and_cancels`，25 秒唤醒计时器或手动唤醒）。后两项涉及打扰桌面，默认忽略、确认后单独运行。
- UIPI 权限受限目标测试（`native_uipi_restricted_target_receives_nothing`）以同一测试二进制的 helper 模式运行两次接收器：普通权限对照组与 UAC 提权组（会弹 UAC 确认）；预期对照组收到全部固定按键，管理员组 SendInput 返回成功但 0 条消息到达。

目标应用最终显示正确不能替代这些事件核对；标准 EDIT 控件通过也不代表所有应用或输入法都兼容。

### 浏览器页面

[receiver.html](receiver.html) 是可选的单文件接收器，记录页面收到的 keydown/keyup、mousedown/mouseup、wheel 的 `performance.now()` 时间与到达顺序，可选记录 mousemove，并导出页面事件 JSON。它只能证明输入到达该页面，浏览器可能合并鼠标移动，不能用页面移动事件数代替原生钩子数量。

## 复现入口

在 Windows 交互桌面、仓库的 `src-tauri/` 目录下运行。以下测试会向真实桌面发送固定输入并切换到专用接收窗口，应单独执行；默认 `cargo test` 会忽略它们。Git Bash 的 MSVC PATH 要求见根 [AGENTS.md](../../AGENTS.md)。

```powershell
cargo test --test native_contract native_capture_edit_save_replay_contract -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_unicode_global_stop_and_cleanup -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_environment_messages_cancel_before_input -- --ignored --nocapture --test-threads=1
cargo test --test native_contract native_keys_edit_preservation -- --ignored --nocapture --test-threads=1
```

需要打扰桌面或用户配合的真实环境测试（确认后单独运行）：

```powershell
# 真实改分辨率约 1 秒并自动恢复（屏幕闪两次）
cargo test --test native_contract native_environment_display_change_cancels -- --ignored --nocapture --test-threads=1
# 真实锁定桌面；测试结束后需要解锁返回
cargo test --test native_contract native_environment_lock_cancels -- --ignored --nocapture --test-threads=1
# 真实睡眠约 25 秒（唤醒计时器不可用时按任意键唤醒）
cargo test --test native_contract native_environment_suspend_resumes_and_cancels -- --ignored --nocapture --test-threads=1
# 弹一次 UAC 启动管理员接收器，验证普通权限注入被 UIPI 静默拦截
cargo test --test native_contract native_uipi_restricted_target_receives_nothing -- --ignored --nocapture --test-threads=1
```

短混合样本使用默认时长。长移动与停顿样本用 `REPEAT_NATIVE_SECONDS=180` 选择；例如在 PowerShell 中：

```powershell
$previousRepeatNativeSeconds = $env:REPEAT_NATIVE_SECONDS
try {
    $env:REPEAT_NATIVE_SECONDS = "180"
    cargo test --test native_contract native_capture_edit_save_replay_contract -- --ignored --nocapture --test-threads=1
} finally {
    $env:REPEAT_NATIVE_SECONDS = $previousRepeatNativeSeconds
}
```

应用联动测试 `native_app_execution_receiver` 还需要通过 `REPEAT_VERIFY_APP_DB` 指定隔离实例的 SQLite 文件，并在该实例中启动三轮执行；它不是上述独立接收测试的自动替代。测试固定样本与输出约定以该文件为准，不对普通用户任务运行批量导出。

## 核对清单

1. 固定输入标记、种类、数量和顺序一致，无溢出或截断；同时记录独立起止时长。
2. 接收侧与当前编译序列对齐，按下/抬起、组合键、滚轮方向与数量符合预期。
3. 开头、中间与结尾停顿保留；等待编辑统一延后后续轨道，循环不额外增加轮间等待。
4. 双击包含两次完整按下/抬起；重复按下与键鼠交错不因动作归并丢失。
5. 失焦时全局停止有效；停止后无新的计划输入，本次仍按住的输入收尾释放，指针不因收尾跳回。
6. 记录偏差、首末误差差值、样本时长、规模和负载；区分引擎注入与目标应用处理。
7. 完整三分钟混合脚本与长移动计时样本分别登记，不互相代替；特殊环境没有实际验证时保留未验收状态。

[标准手动样本](sample-script.md)规定混合操作步骤；[实施计划](../implementation-plan.md)维护实测结果、内部目标与交付边界。键盘编辑末轮检查是否执行以实施计划记录为准。
