//! 保存失败后从界面重试的实机链路。隔离实例真实录制，测试进程持有 SQLite 写锁
//! 模拟保存失败，经 UIA 读取界面横幅文本与"重试保存"按钮矩形（WebView2 以
//! --force-renderer-accessibility 暴露可访问性树），真实点击触发界面重试，再重开
//! 实例核对动作与录像。显式 --ignored 才运行；运行期间保持键鼠空闲。
#![cfg(windows)]

use repeat_lib::{
    capture::Hotkeys,
    document::{RawRecording, TaskDocument},
    event::{EventKind, RecordedEvent},
    platform,
    storage::Store,
};
use rusqlite::Connection;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::{
            Input::KeyboardAndMouse::*,
            WindowsAndMessaging::{
                BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
                FindWindowExW, GetForegroundWindow, GetMessageW, GetWindowRect,
                GetWindowThreadProcessId, HWND_TOP, IsWindowVisible, MSG, PostMessageW,
                PostQuitMessage, RegisterClassW, SW_RESTORE, SWP_NOACTIVATE, SWP_NOSIZE,
                SendMessageW, SetCursorPos, SetForegroundWindow, SetWindowPos, ShowWindow,
                TranslateMessage, UnregisterClassW, WM_APP, WM_CLOSE, WM_DESTROY, WNDCLASSW,
                WS_OVERLAPPEDWINDOW, WS_VISIBLE,
            },
        },
    },
    core::{PCWSTR, w},
};

/// UIA 探针：读取"重试保存"按钮物理矩形与含关键文案的元素名。
/// WebView2 需 --force-renderer-accessibility 才渲染可访问性树。
const UIA_PROBE_SCRIPT: &str = "\u{FEFF}param([int]$HwndPtr)
[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName UIAutomationClient
Add-Type -AssemblyName UIAutomationTypes
$root = [System.Windows.Automation.AutomationElement]::FromHandle([IntPtr]$HwndPtr)
if ($root -eq $null) { Write-Output 'WINDOW_ELEMENT='; exit }
$cond = New-Object System.Windows.Automation.PropertyCondition([System.Windows.Automation.AutomationElement]::NameProperty, '重试保存')
$btn = $root.FindFirst([System.Windows.Automation.TreeScope]::Descendants, $cond)
if ($btn -eq $null) { Write-Output 'BUTTON_RECT=' } else {
  $b = $btn.Current.BoundingRectangle
  Write-Output ('BUTTON_RECT={0},{1},{2},{3}' -f [int]$b.X, [int]$b.Y, ([int]($b.X+$b.Width)), ([int]($b.Y+$b.Height)))
}
$all = $root.FindAll([System.Windows.Automation.TreeScope]::Descendants, [System.Windows.Automation.Condition]::TrueCondition)
$texts = @()
foreach ($e in $all) { $n = $e.Current.Name; if ($n -and $n -match '尚未保存|原任务|失败') { $texts += $n } }
Write-Output ('BANNER_TEXT=' + ($texts -join ' | '))";

struct ChildCleanup {
    process: std::process::Child,
}
impl Drop for ChildCleanup {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

struct Receiver {
    hwnd: usize,
    join: Option<thread::JoinHandle<()>>,
}
impl Drop for Receiver {
    fn drop(&mut self) {
        unsafe {
            let _ = PostMessageW(
                Some(HWND(self.hwnd as *mut _)),
                WM_CLOSE,
                WPARAM(0),
                LPARAM(0),
            );
        }
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

unsafe extern "system" fn receiver_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_APP + 12 {
        unsafe {
            let foreground_thread = GetWindowThreadProcessId(GetForegroundWindow(), None);
            let own = windows::Win32::System::Threading::GetCurrentThreadId();
            let attached =
                windows::Win32::System::Threading::AttachThreadInput(own, foreground_thread, true)
                    .as_bool();
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(Some(hwnd));
            if attached {
                let _ = windows::Win32::System::Threading::AttachThreadInput(
                    own,
                    foreground_thread,
                    false,
                );
            }
        }
        return LRESULT(0);
    }
    if msg == WM_CLOSE {
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
        return LRESULT(0);
    }
    if msg == WM_DESTROY {
        unsafe {
            PostQuitMessage(0);
        }
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

fn receiver_new() -> Receiver {
    let (tx, rx) = std::sync::mpsc::channel();
    let join = thread::spawn(move || unsafe {
        let instance: HINSTANCE = GetModuleHandleW(None).unwrap().into();
        let class = WNDCLASSW {
            lpfnWndProc: Some(receiver_proc),
            hInstance: instance,
            lpszClassName: w!("RepeatSaveRetryReceiver"),
            ..Default::default()
        };
        assert_ne!(RegisterClassW(&class), 0);
        let hwnd = CreateWindowExW(
            Default::default(),
            w!("RepeatSaveRetryReceiver"),
            w!("Repeat 保存重试验证接收器"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            150,
            150,
            800,
            560,
            None,
            None,
            Some(instance),
            None,
        )
        .unwrap();
        let _ = tx.send(hwnd.0 as usize);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = UnregisterClassW(w!("RepeatSaveRetryReceiver"), Some(instance));
    });
    let hwnd = rx
        .recv_timeout(Duration::from_secs(5))
        .expect("接收器创建超时");
    thread::sleep(Duration::from_millis(150));
    Receiver {
        hwnd,
        join: Some(join),
    }
}

fn focus_receiver(receiver: &Receiver) {
    unsafe {
        SetWindowPos(
            HWND(receiver.hwnd as *mut _),
            Some(HWND_TOP),
            150,
            150,
            800,
            560,
            SWP_NOACTIVATE,
        )
        .unwrap();
        let _ = SendMessageW(HWND(receiver.hwnd as *mut _), WM_APP + 12, None, None);
    }
    thread::sleep(Duration::from_millis(150));
}

/// 把主窗口恢复显示、还原最小化并移到已知位置；返回其物理矩形。
fn restore_main(hwnd: HWND) -> RECT {
    unsafe {
        let _ = ShowWindow(hwnd, SW_RESTORE);
        SetWindowPos(
            hwnd,
            Some(HWND_TOP),
            100,
            100,
            0,
            0,
            SWP_NOACTIVATE | SWP_NOSIZE,
        )
        .expect("移动主窗口失败");
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect).expect("获取主窗口矩形失败");
        rect
    }
}

fn wait_window(pid: u32, title: PCWSTR, timeout: Duration, what: &str) -> HWND {
    let started = Instant::now();
    loop {
        let mut previous = None;
        while let Ok(hwnd) = unsafe { FindWindowExW(None, previous, None, title) } {
            let mut owner_pid = 0;
            unsafe { GetWindowThreadProcessId(hwnd, Some(&mut owner_pid)) };
            if owner_pid == pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
                return hwnd;
            }
            previous = Some(hwnd);
        }
        assert!(started.elapsed() < timeout, "等待窗口超时：{what}");
        thread::sleep(Duration::from_millis(50));
    }
}

/// 无标记注入：录制钩子按 REPEAT_INJECT_MARKER 过滤引擎自身事件，
/// 这里不带标记，使合成输入被当作真实输入录进样本。
fn send_key(vk: u32, scan: u16, down: bool) {
    let mut input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk as u16),
                wScan: scan,
                time: 0,
                dwExtraInfo: 0,
                ..Default::default()
            },
        },
    };
    if down {
        input.Anonymous.ki.dwFlags = KEYBD_EVENT_FLAGS(0);
    } else {
        input.Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
    }
    assert_eq!(
        unsafe { SendInput(&[input], size_of::<INPUT>() as i32) },
        1,
        "键盘注入失败"
    );
    thread::sleep(Duration::from_millis(60));
}

fn send_click() {
    let down = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: MOUSEEVENTF_LEFTDOWN,
                ..Default::default()
            },
        },
    };
    let up = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dwFlags: MOUSEEVENTF_LEFTUP,
                ..Default::default()
            },
        },
    };
    thread::sleep(Duration::from_millis(60));
    assert_eq!(
        unsafe { SendInput(&[down, up], size_of::<INPUT>() as i32) },
        2,
        "鼠标点击注入失败"
    );
    thread::sleep(Duration::from_millis(60));
}

fn send_move(dx: i32, dy: i32) {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                dwFlags: MOUSEEVENTF_MOVE,
                ..Default::default()
            },
        },
    };
    assert_eq!(
        unsafe { SendInput(&[input], size_of::<INPUT>() as i32) },
        1,
        "鼠标移动注入失败"
    );
    thread::sleep(Duration::from_millis(100));
}

fn click_at(x: i32, y: i32) {
    unsafe {
        SetCursorPos(x, y).expect("移动光标失败");
    }
    thread::sleep(Duration::from_millis(120));
    send_click();
}

/// 把主窗口压到前台（attach 前台线程的焦点舞步），供真实点击落在其按钮上。
fn bring_to_front(hwnd: HWND) {
    unsafe {
        let foreground = GetForegroundWindow();
        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let own = GetCurrentThreadId();
        let attached =
            windows::Win32::System::Threading::AttachThreadInput(own, foreground_thread, true)
                .as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ =
                windows::Win32::System::Threading::AttachThreadInput(own, foreground_thread, false);
        }
    }
    thread::sleep(Duration::from_millis(250));
}

/// UIA 探针：返回 (按钮物理矩形 Option<(i32,i32,i32,i32)>, 界面关键文案)。
fn uia_probe(script: &Path, hwnd: HWND) -> (Option<(i32, i32, i32, i32)>, String) {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
            &script.to_string_lossy(),
            "-HwndPtr",
            &(hwnd.0 as usize).to_string(),
        ])
        .output()
        .expect("无法运行 UIA 探针");
    assert!(
        output.status.success(),
        "UIA 探针失败：{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut button = None;
    let mut banner = String::new();
    for line in stdout.lines() {
        if let Some(rest) = line.strip_prefix("BUTTON_RECT=")
            && !rest.is_empty()
        {
            let parts: Vec<_> = rest.split(',').map(|v| v.parse::<i32>().unwrap()).collect();
            button = Some((parts[0], parts[1], parts[2], parts[3]));
        }
        if let Some(rest) = line.strip_prefix("BANNER_TEXT=") {
            banner = rest.to_string();
        }
    }
    (button, banner)
}

fn document_json(conn: &Connection) -> serde_json::Value {
    let (revision, document): (i64, String) = conn
        .query_row(
            "SELECT revision, document FROM tasks ORDER BY rowid LIMIT 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("读取任务行失败");
    let mut value: serde_json::Value = serde_json::from_str(&document).unwrap();
    value["revision"] = revision.into();
    value
}

#[test]
#[ignore = "隔离实例真实录制并模拟保存失败；向真实桌面注入键鼠与一次界面点击，运行期间请保持键鼠空闲"]
fn native_save_failure_ui_retry() {
    platform::init_process_dpi();
    let executable = std::env::var_os("REPEAT_E2E_EXECUTABLE")
        .expect("REPEAT_E2E_EXECUTABLE 须指向隔离测试使用的 Repeat 可执行文件");
    let work_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/save-retry-evidence");
    std::fs::create_dir_all(&work_dir).unwrap();
    let script = work_dir.join("uia-probe.ps1");
    std::fs::write(&script, UIA_PROBE_SCRIPT).unwrap();
    let data_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("save-retry-e2e-{}", std::process::id()));
    std::fs::create_dir(&data_dir).expect("测试目录已存在或无法创建");
    let geometry = platform::snapshot_geometry();

    // 原始任务：少量固定事件，修订 1。保存失败时它必须原样保留。
    let mut doc = TaskDocument::new(
        "保存重试固定样本".into(),
        geometry.clone(),
        platform::current_layout_klid(),
    );
    let original: Vec<(EventKind, i64)> = vec![
        (
            EventKind::MouseMove {
                x: geometry.monitors[0].x + 300,
                y: geometry.monitors[0].y + 300,
            },
            200_000,
        ),
        (
            EventKind::MouseMove {
                x: geometry.monitors[0].x + 320,
                y: geometry.monitors[0].y + 320,
            },
            500_000,
        ),
    ];
    let events = original
        .into_iter()
        .enumerate()
        .map(|(index, (kind, time_us))| RecordedEvent {
            seq: index as u64 + 1,
            time_us,
            native_time_ms: 0,
            kind,
        })
        .collect();
    doc.replace_recording(RawRecording {
        gestures: Default::default(),
        started_at_wall_ms: 0,
        duration_us: 1_000_000,
        events,
        geometry,
        layout_klid: platform::current_layout_klid(),
    });
    doc.validate_execution().unwrap();
    let mut store = Store::open(&data_dir.join("tasks.sqlite3")).unwrap();
    store.insert(&doc).unwrap();
    store
        .set_setting(
            "hotkeys",
            &serde_json::to_string(&Hotkeys {
                record_vk: 0x79,
                stop_vk: 0x7a,
            })
            .unwrap(),
        )
        .unwrap();

    // stdio 置空：观察/收尾阶段实例可能独立存活，不得占住 cargo 的输出管道。
    let child = Command::new(&executable)
        .env("REPEAT_DATA_DIR", &data_dir)
        .env(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--force-renderer-accessibility",
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("无法启动隔离 Repeat");
    let mut child = ChildCleanup { process: child };
    let pid = child.process.id();
    println!("SAVE_RETRY_READY pid={pid} data={}", data_dir.display());
    let main_hwnd = wait_window(pid, w!("Repeat"), Duration::from_secs(60), "主窗口");
    thread::sleep(Duration::from_millis(1200));

    // 持有 RESERVED 写锁：应用的 store.save 将等待 busy_timeout 后报锁定。
    let conn = Connection::open(data_dir.join("tasks.sqlite3")).unwrap();
    conn.execute_batch("PRAGMA busy_timeout=2000; BEGIN IMMEDIATE; SELECT count(*) FROM tasks;")
        .unwrap();

    // F10 经 RegisterHotKey 启动录制准备（3 秒倒计时后开始捕获）。
    send_key(0x79, 0x44, true);
    send_key(0x79, 0x44, false);
    wait_window(
        pid,
        w!("Repeat 执行状态"),
        Duration::from_secs(15),
        "录制状态条",
    );
    wait_window(
        pid,
        w!("Repeat 执行范围"),
        Duration::from_secs(15),
        "录制边框（捕获已开始）",
    );
    println!("SAVE_RETRY_RECORDING_STARTED");

    // 接收器取得焦点作为注入落点，录制固定内容：移动、左键单击、三次 A。
    let receiver = receiver_new();
    let focus_deadline = Instant::now();
    loop {
        focus_receiver(&receiver);
        if unsafe { GetForegroundWindow() }.0 as usize == receiver.hwnd {
            break;
        }
        assert!(
            focus_deadline.elapsed() < Duration::from_secs(8),
            "接收器无法取得前台焦点（运行期间请勿操作键鼠）"
        );
        thread::sleep(Duration::from_millis(200));
    }
    let mut center = RECT::default();
    unsafe { GetWindowRect(HWND(receiver.hwnd as *mut _), &mut center).unwrap() };
    unsafe {
        SetCursorPos(
            (center.left + center.right) / 2,
            (center.top + center.bottom) / 2,
        )
        .unwrap()
    };
    for delta in [40, -30, 25, -20, 15] {
        send_move(delta, -delta / 2);
    }
    send_click();
    for _ in 0..3 {
        send_key(0x41, 0x1E, true);
        send_key(0x41, 0x1E, false);
    }
    thread::sleep(Duration::from_millis(500));

    // 停止录制：再按一次 F10（record_vk）。录制期停止走 WM_HOTKEY→ToggleRecording，
    // 注册热键响应注入输入；F11/stop_vk 的钩子物理路径只覆盖执行停止与准备期取消。
    send_key(0x79, 0x44, true);
    send_key(0x79, 0x44, false);
    println!("SAVE_RETRY_STOP_SENT：等待录制停止与保存失败（busy_timeout 5 秒）");
    let stop_deadline = Instant::now() + Duration::from_secs(20);
    loop {
        // 主窗口在录制开始时隐藏、停止后恢复显示，以其可见作为停止信号。
        if unsafe { IsWindowVisible(main_hwnd) }.as_bool() {
            break;
        }
        assert!(
            Instant::now() < stop_deadline,
            "再次 F10 后录制未停止（主窗口未恢复显示）"
        );
        thread::sleep(Duration::from_millis(100));
    }
    thread::sleep(Duration::from_millis(8000));
    conn.execute_batch("ROLLBACK").unwrap();

    // 原任务保留：修订与事件数不变；录像文件已收尾重命名为正式名但未入任务。
    let failed_doc = document_json(&conn);
    assert_eq!(failed_doc["revision"], 1, "保存失败后任务修订不应变化");
    assert_eq!(
        failed_doc["raw"]["events"].as_array().map(|a| a.len()),
        Some(2),
        "保存失败后原任务事件不应变化：{failed_doc:?}"
    );
    let recordings_dir = data_dir.join("recordings");
    let media: Vec<PathBuf> = std::fs::read_dir(&recordings_dir)
        .unwrap()
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .collect();
    assert_eq!(media.len(), 1, "录制应恰好产生一个 mp4 文件：{media:?}");
    assert!(
        failed_doc["screenRecording"].is_null(),
        "保存失败后原任务不应引用新录像：{failed_doc:?}"
    );
    println!(
        "SAVE_RETRY_SAVE_FAILED preserved=true media={}",
        media[0].display()
    );

    // 界面证据：主窗口恢复（可能回到最小化），先还原到已知位置再读横幅与按钮。
    let rect = restore_main(main_hwnd);
    println!(
        "SAVE_RETRY_MAIN_RESTORED rect=({},{},{},{})",
        rect.left, rect.top, rect.right, rect.bottom
    );
    let banner_deadline = Instant::now() + Duration::from_secs(15);
    let (button, banner_text) = loop {
        let (button, banner) = uia_probe(&script, main_hwnd);
        if button.is_some() {
            break (button, banner);
        }
        assert!(
            Instant::now() < banner_deadline,
            "界面上未找到'重试保存'按钮（横幅文案：{banner:?}）"
        );
        thread::sleep(Duration::from_millis(500));
    };
    assert!(
        banner_text.contains("原任务保持不变") || banner_text.contains("尚未保存"),
        "界面横幅缺少待保存提示文案：{banner_text:?}"
    );
    let (bx1, by1, bx2, by2) = button.unwrap();
    println!("SAVE_RETRY_BANNER text={banner_text:?} button=({bx1},{by1})-({bx2},{by2})");

    // 通过界面重试：真实点击"重试保存"。
    bring_to_front(main_hwnd);
    click_at((bx1 + bx2) / 2, (by1 + by2) / 2);
    println!(
        "SAVE_RETRY_CLICKED at=({}, {})",
        (bx1 + bx2) / 2,
        (by1 + by2) / 2
    );
    thread::sleep(Duration::from_millis(2500));

    let retried_doc = document_json(&conn);
    assert_eq!(
        retried_doc["revision"], 2,
        "重试后任务修订应为 2：{retried_doc:?}"
    );
    let events = retried_doc["raw"]["events"]
        .as_array()
        .expect("重试后应有录制事件")
        .clone();
    assert!(
        events.len() >= 11,
        "录制事件数不足（5 移动+1 点击+3 A=9 起）：{}",
        events.len()
    );
    let has_a_key = events
        .iter()
        .any(|e| e["kind"]["kind"] == "keyDown" && e["kind"]["vk"] == 65);
    let has_click = events.iter().any(|e| e["kind"]["kind"] == "mouseDown");
    assert!(
        has_a_key && has_click,
        "录制内容缺少注入的按键或点击：{retried_doc:?}"
    );
    let media_id = retried_doc["screenRecording"]["id"]
        .as_str()
        .expect("重试后任务应引用新录像")
        .to_string();
    let media_path = recordings_dir.join(format!("{media_id}.mp4"));
    assert!(
        media_path.exists(),
        "任务引用的录像文件缺失：{media_path:?}"
    );
    let duration_us = retried_doc["screenRecording"]["durationUs"]
        .as_i64()
        .expect("录像时长缺失");
    println!(
        "SAVE_RETRY_SAVED revision=2 events={} mediaId={media_id} durationUs={duration_us}",
        events.len()
    );

    // 重试后横幅消失。
    let (button_after, _) = uia_probe(&script, main_hwnd);
    assert!(button_after.is_none(), "重试保存后横幅按钮仍存在");

    // 重开：优雅退出后重启隔离实例，核对任务与录像仍可读。
    unsafe {
        PostMessageW(Some(main_hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).expect("投递 WM_CLOSE 失败");
    }
    let close_sent = Instant::now();
    loop {
        if child
            .process
            .try_wait()
            .expect("查询子进程状态失败")
            .is_some()
        {
            break;
        }
        assert!(
            close_sent.elapsed() < Duration::from_secs(20),
            "重开阶段关闭主窗口后进程未退出"
        );
        thread::sleep(Duration::from_millis(20));
    }
    let relaunched = Command::new(&executable)
        .env("REPEAT_DATA_DIR", &data_dir)
        .env(
            "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
            "--force-renderer-accessibility",
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("无法重启隔离 Repeat");
    let relaunched = ChildCleanup {
        process: relaunched,
    };
    let reopened_hwnd = wait_window(
        relaunched.process.id(),
        w!("Repeat"),
        Duration::from_secs(60),
        "重开后的主窗口",
    );
    thread::sleep(Duration::from_millis(1500));
    let reopened_store = Store::open(&data_dir.join("tasks.sqlite3")).unwrap();
    let active = reopened_store.setting("activeTask").unwrap().unwrap();
    let reopened_doc = reopened_store.load(&active).unwrap();
    assert_eq!(reopened_doc.revision, 2, "重开后任务修订应为 2");
    assert_eq!(
        reopened_doc.raw.as_ref().unwrap().events.len(),
        events.len(),
        "重开后动作事件数不一致"
    );
    assert!(
        reopened_doc.screen_recording.is_some(),
        "重开后应保留录像引用"
    );
    let probe = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "default=nw=1:nk=1",
        ])
        .arg(&media_path)
        .output()
        .expect("无法运行 ffprobe");
    assert!(
        probe.status.success(),
        "录像无法解码：{}",
        String::from_utf8_lossy(&probe.stderr)
    );
    let seconds: f64 = String::from_utf8_lossy(&probe.stdout)
        .trim()
        .parse()
        .unwrap();
    let expected_seconds = duration_us as f64 / 1_000_000.0;
    assert!(
        (seconds - expected_seconds).abs() < 1.5,
        "录像时长 {seconds}s 与记录时长 {expected_seconds}s 不符"
    );
    println!("SAVE_RETRY_REOPENED revision=2 mp4={seconds:.2}s");
    let _ = reopened_hwnd;

    let evidence = serde_json::json!({
        "fixture": "isolated Repeat; real recording via record hotkey; SQLite RESERVED lock simulates save failure; UIA-located real UI click retries; reopen verified",
        "dataDir": data_dir,
        "pid": pid,
        "originalRevision": 1,
        "originalEvents": 2,
        "mediaFile": media[0],
        "bannerText": banner_text,
        "retryButtonRect": [bx1, by1, bx2, by2],
        "retriedRevision": 2,
        "retriedEvents": events.len(),
        "mediaId": media_id,
        "recordedDurationUs": duration_us,
        "mp4DurationSeconds": seconds,
    });
    std::fs::write(
        work_dir.join("evidence.json"),
        serde_json::to_vec_pretty(&evidence).unwrap(),
    )
    .unwrap();
    println!("SAVE_RETRY_EVIDENCE {evidence}");
}
