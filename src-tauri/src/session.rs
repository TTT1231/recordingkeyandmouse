//! 原生会话：控制、任务事务、编辑修订和可取消的系统执行。
use crate::capture::RecordingBuffer;
use crate::document::{
    DocumentView, EditRequest, RawRecording, TaskDocument, TaskSummary, wall_ms,
};
use crate::event::{EventKind, MouseButton};
use crate::platform::{self, HotkeyAction};
use crate::scheduler::{
    ExecutionFailure, ExecutionOutcome, LATE_THRESHOLD, PacerOutcome, ProgressEvent, ReplayDriver,
    ScheduleSpec, StopSignal, TimingStats, compute_stats, run_replay, scale_execution_time,
};
use crate::storage::{Hotkeys, Store};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

/// u32::MAX 是“无限循环”的调度哨兵；只能由停止按钮或停止热键结束。
pub const INFINITE_ROUNDS: u32 = u32::MAX;

struct RecordingLive {
    capture_id: u64,
    gestures: crate::document::GestureSettings,
    buffer: Arc<Mutex<RecordingBuffer>>,
    collected: Vec<crate::event::RecordedEvent>,
    geometry: crate::geometry::DesktopGeometry,
    layout_klid: String,
    started_at_wall_ms: i64,
}
#[derive(Default)]
struct ExecutionProgress {
    phase: AtomicU8,
    round: AtomicU32,
    injected: AtomicU64,
    last_seq: AtomicU64,
    round_started: Mutex<Option<Instant>>,
}
struct ExecutionLive {
    stop: Arc<StopSignal>,
    rounds: u32,
    speed: f64,
    progress: Arc<ExecutionProgress>,
    prepare_until: Instant,
}
enum Mode {
    Idle,
    Recording(RecordingLive),
    Executing(ExecutionLive),
}
#[derive(Default)]
struct History {
    past: Vec<TaskDocument>,
    future: Vec<TaskDocument>,
}
struct SessionState {
    store: Store,
    current: TaskDocument,
    demo: Option<TaskDocument>,
    histories: BTreeMap<String, History>,
    mode: Mode,
    report: Option<ExecutionReportBundle>,
    last_error: Option<String>,
    pending: Option<TaskDocument>,
    hotkeys: Hotkeys,
}
impl SessionState {
    fn doc(&self) -> &TaskDocument {
        self.demo.as_ref().unwrap_or(&self.current)
    }
    fn ensure_idle(&self) -> Result<(), String> {
        if matches!(self.mode, Mode::Idle) {
            Ok(())
        } else {
            Err("请先停止录制或系统执行，再修改任务".into())
        }
    }
    fn check(&self, id: &str, revision: u64) -> Result<(), String> {
        if self.doc().id == id && self.doc().revision == revision {
            Ok(())
        } else {
            Err("任务或修订已经变化，旧操作未执行，请重新选择".into())
        }
    }
    fn commit(&mut self, mut next: TaskDocument) -> Result<(), String> {
        let old = self.doc().clone();
        next.revision = old.revision.checked_add(1).ok_or("任务修订号超出范围")?;
        next.updated_at = wall_ms();
        if self.demo.is_none() {
            self.store.save(&next, old.revision)?;
        }
        let history = self.histories.entry(next.id.clone()).or_default();
        history.past.push(old);
        history.future.clear();
        if self.demo.is_some() {
            self.demo = Some(next);
        } else {
            self.current = next;
        }
        self.last_error = None;
        Ok(())
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingLiveView {
    pub elapsed_us: i64,
    pub event_count: usize,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionLiveView {
    pub rounds_requested: u32,
    pub current_round: u32,
    pub events_injected: u64,
    pub last_seq: u64,
    pub stop_requested: bool,
    pub prepare_remaining_ms: u64,
    pub elapsed_us: i64,
    pub speed: f64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub version: u64,
    pub mode: &'static str,
    pub task_id: String,
    pub revision: u64,
    pub has_recording: bool,
    pub recording_live: Option<RecordingLiveView>,
    pub execution_live: Option<ExecutionLiveView>,
    pub has_execution_report: bool,
    pub last_error: Option<String>,
    pub hotkey_status: Vec<String>,
    pub hotkeys: Hotkeys,
    pub can_undo: bool,
    pub can_redo: bool,
    pub is_demo: bool,
    pub has_pending_recording: bool,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionReportBundle {
    pub outcome: ExecutionOutcome,
    pub stats: TimingStats,
    pub planned_rounds: u32,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceView {
    pub session: SessionSnapshot,
    pub document: DocumentView,
    pub tasks: Vec<TaskSummary>,
    pub data_path: String,
}

pub struct AppSession {
    inner: Mutex<SessionState>,
    service: OnceLock<platform::InputServiceHandle>,
    executor: Mutex<Option<JoinHandle<()>>>,
    controller: Mutex<Option<JoinHandle<()>>>,
    closed: AtomicBool,
    next_snapshot: AtomicU64,
    execution_stop: Mutex<Option<Arc<StopSignal>>>,
    data_path: String,
}
impl AppSession {
    pub fn new(path: &Path) -> Result<Arc<Self>, String> {
        let mut store = Store::open(path)?;
        let tasks = store.list()?;
        let active = store
            .setting("activeTask")?
            .and_then(|id| tasks.iter().find(|t| t.id == id).map(|t| t.id.clone()))
            .or_else(|| tasks.first().map(|t| t.id.clone()));
        let current = if let Some(id) = active {
            store.load(&id)?
        } else {
            let doc = TaskDocument::new(
                "我的任务".into(),
                platform::snapshot_geometry(),
                platform::current_layout_klid(),
            );
            store.insert(&doc)?;
            doc
        };
        let hotkeys = store.hotkeys()?;
        Ok(Arc::new(Self {
            inner: Mutex::new(SessionState {
                store,
                current,
                demo: None,
                histories: BTreeMap::new(),
                mode: Mode::Idle,
                report: None,
                last_error: None,
                pending: None,
                hotkeys,
            }),
            service: OnceLock::new(),
            executor: Mutex::new(None),
            controller: Mutex::new(None),
            closed: AtomicBool::new(false),
            next_snapshot: AtomicU64::new(1),
            execution_stop: Mutex::new(None),
            data_path: path.display().to_string(),
        }))
    }
    pub fn init(self: &Arc<Self>, app: &AppHandle) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        let weak = Arc::downgrade(self);
        let handle = match platform::start_input_service(Arc::new(move |action| {
            if matches!(
                action,
                HotkeyAction::StopExecution | HotkeyAction::EnvironmentChanged
            ) && let Some(session) = weak.upgrade()
                && let Some(stop) = session.execution_stop.lock().unwrap().as_ref()
            {
                if action == HotkeyAction::EnvironmentChanged {
                    stop.fail("桌面会话、显示布局或电源状态发生变化，已停止执行");
                } else {
                    stop.signal();
                }
            }
            let _ = tx.send(action);
        })) {
            Ok(handle) => handle,
            Err(reason) => {
                self.inner.lock().unwrap().last_error = Some(reason);
                return Ok(());
            }
        };
        let config = self.inner.lock().unwrap().hotkeys.clone();
        if let Err(reason) = handle.set_hotkeys(config) {
            self.inner.lock().unwrap().last_error = Some(reason);
        }
        self.service.set(handle).map_err(|_| "输入服务已经初始化")?;
        let session = self.clone();
        let app = app.clone();
        *self.controller.lock().unwrap() = Some(
            std::thread::Builder::new()
                .name("repeat-controls".into())
                .spawn(move || {
                    while !session.closed.load(Ordering::SeqCst) {
                        match rx.recv_timeout(Duration::from_millis(100)) {
                            Ok(HotkeyAction::EnvironmentChanged) => {
                                let mut st = session.inner.lock().unwrap();
                                if matches!(st.mode, Mode::Recording(_)) {
                                    let _ = session.input_service().and_then(|s|s.stop_capture());
                                    st.mode = Mode::Idle;
                                    st.last_error = Some("桌面会话、显示布局或电源状态发生变化，本次录制未替换原任务".into());
                                    crate::overlay::hide(&app);
                                }
                                if let Mode::Executing(live)=&st.mode {live.stop.fail("输入服务或桌面环境发生变化，已停止执行");}
                                drop(st);
                                session.emit_state(&app);
                            }
                            Ok(HotkeyAction::ToggleRecording) => {
                                let (id, revision, recording, busy, demo) = {
                                    let st = session.inner.lock().unwrap();
                                    (
                                        st.current.id.clone(),
                                        st.current.revision,
                                        matches!(st.mode, Mode::Recording(_)),
                                        matches!(st.mode, Mode::Executing(_)),
                                        st.demo.is_some(),
                                    )
                                };
                                if busy || demo {
                                    continue;
                                }
                                let result = if recording {
                                    session.stop_recording(&app, None)
                                } else {
                                    session.start_recording(&app, &id, revision)
                                };
                                if let Err(reason) = result {
                                    session.set_error(&app, reason);
                                }
                            }
                            Ok(HotkeyAction::StopExecution) => {
                                // 钩子回调已直接通知当次执行；队列可能延迟到下次执行才排空。
                                session.emit_state(&app);
                            }
                            Ok(HotkeyAction::StopRecording(capture_id)) => {
                                if let Err(reason) = session.stop_recording_checked(&app, None, Some(capture_id)) {
                                    session.set_error(&app, reason);
                                }
                            }
                            Err(mpsc::RecvTimeoutError::Timeout) => {}
                            Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        }
                        session.drain_capture(&app);
                    }
                })
                .map_err(|e| format!("启动控制服务失败：{e}"))?,
        );
        Ok(())
    }
    fn input_service(&self) -> Result<&platform::InputServiceHandle, String> {
        self.service
            .get()
            .ok_or_else(|| "当前系统输入服务不可用".into())
    }
    pub fn shutdown(&self, app: &AppHandle) {
        self.closed.store(true, Ordering::SeqCst);
        let recording = {
            let st = self.inner.lock().unwrap();
            if let Mode::Executing(live) = &st.mode {
                live.stop.signal();
            }
            matches!(st.mode, Mode::Recording(_))
        };
        if recording {
            let _ = self.stop_recording(app, None);
        }
        if let Some(handle) = self.executor.lock().unwrap().take() {
            let _ = handle.join();
        }
        if let Some(service) = self.service.get() {
            service.shutdown();
        }
        if let Some(handle) = self.controller.lock().unwrap().take() {
            let _ = handle.join();
        }
    }
    fn set_error(&self, app: &AppHandle, reason: String) {
        self.inner.lock().unwrap().last_error = Some(reason);
        self.emit_state(app);
    }
    pub fn clear_error(&self, app: &AppHandle) {
        self.inner.lock().unwrap().last_error = None;
        self.emit_state(app);
    }
    fn drain_capture(&self, app: &AppHandle) {
        let mut st = self.inner.lock().unwrap();
        let error = if let Mode::Recording(live) = &mut st.mode {
            let (pending, overflow) = {
                let mut buffer = live.buffer.lock().unwrap();
                (buffer.drain_pending(), buffer.overflowed())
            };
            if overflow {
                Some("捕获队列溢出，本次录制不完整，原任务已保留".to_string())
            } else if live.collected.try_reserve(pending.len()).is_err() {
                Some("内存不足，本次录制未保存，原任务已保留".into())
            } else {
                live.collected.extend(pending);
                None
            }
        } else {
            None
        };
        if let Some(reason) = error {
            if let Some(service) = self.service.get() {
                let _ = service.stop_capture();
            }
            st.mode = Mode::Idle;
            st.last_error = Some(reason);
            crate::overlay::hide(app);
            drop(st);
            self.emit_state(app);
        }
    }
    pub fn start_recording(
        &self,
        app: &AppHandle,
        task_id: &str,
        revision: u64,
    ) -> Result<(), String> {
        let mut st = self.inner.lock().unwrap();
        st.ensure_idle()?;
        st.check(task_id, revision)?;
        if st.demo.is_some() {
            return Err("演示不会录制系统输入，请退出演示后开始录制".into());
        }
        if st.pending.is_some() {
            return Err("请先重试保存或放弃上次未保存的录制".into());
        }
        let geometry = platform::snapshot_geometry();
        let buffer = Arc::new(Mutex::new(RecordingBuffer::new(st.hotkeys.vks())));
        buffer.lock().unwrap().begin();
        let capture_id = self.input_service()?.start_capture(buffer.clone())?;
        st.mode = Mode::Recording(RecordingLive {
            capture_id,
            gestures: platform::gesture_settings(),
            buffer,
            collected: Vec::new(),
            geometry,
            layout_klid: platform::current_layout_klid(),
            started_at_wall_ms: wall_ms(),
        });
        if let Err(reason) = crate::overlay::show_recording(app) {
            let _ = self.input_service()?.stop_capture();
            st.mode = Mode::Idle;
            return Err(reason);
        }
        st.last_error = None;
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn stop_recording(&self, app: &AppHandle, gesture: Option<&str>) -> Result<(), String> {
        self.stop_recording_checked(app, gesture, None)
    }
    fn stop_recording_checked(
        &self,
        app: &AppHandle,
        gesture: Option<&str>,
        capture_id: Option<u64>,
    ) -> Result<(), String> {
        let mut st = self.inner.lock().unwrap();
        if let Some(expected) = capture_id
            && !matches!(&st.mode, Mode::Recording(live) if live.capture_id == expected)
        {
            return Ok(());
        }
        if !matches!(st.mode, Mode::Recording(_)) {
            return Err("当前没有进行中的录制".into());
        }
        self.input_service()?.stop_capture()?;
        let Mode::Recording(mut live) = std::mem::replace(&mut st.mode, Mode::Idle) else {
            unreachable!()
        };
        crate::overlay::hide(app);
        let finished = live.buffer.lock().unwrap().finish();
        if finished.overflow {
            st.last_error = Some("捕获队列溢出，本次录制未替换原任务".into());
            drop(st);
            self.emit_state(app);
            return Ok(());
        }
        if live.collected.try_reserve(finished.events.len()).is_err() {
            st.last_error = Some("内存不足，录制未能保存，原任务仍保留".into());
            drop(st);
            self.emit_state(app);
            return Ok(());
        }
        live.collected.extend(finished.events);
        let duration = crate::capture::trim_control_gesture(
            &mut live.collected,
            finished.duration_us,
            gesture,
        );
        let mut next = st.current.clone();
        next.replace_recording(RawRecording {
            gestures: live.gestures,
            events: live.collected,
            duration_us: duration,
            geometry: live.geometry,
            layout_klid: live.layout_klid,
            started_at_wall_ms: live.started_at_wall_ms,
        });
        match st.commit(next.clone()) {
            Ok(()) => {}
            Err(reason) => {
                st.pending = Some(next);
                st.last_error = Some(format!("录制尚未保存，原任务仍保留。{reason}"));
            }
        }
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn cancel_recording(&self, app: &AppHandle, id: &str, revision: u64) -> Result<(), String> {
        let mut st = self.inner.lock().unwrap();
        st.check(id, revision)?;
        if !matches!(st.mode, Mode::Recording(_)) {
            return Err("当前没有进行中的录制".into());
        }
        self.input_service()?.stop_capture()?;
        st.mode = Mode::Idle;
        st.last_error = None;
        crate::overlay::hide(app);
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn retry_recording(&self, app: &AppHandle, discard: bool) -> Result<(), String> {
        let mut st = self.inner.lock().unwrap();
        st.ensure_idle()?;
        if !discard {
            let next = st.pending.clone().ok_or("没有待保存录制")?;
            st.commit(next)?;
        }
        st.pending = None;
        st.last_error = None;
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn create_task(&self, app: &AppHandle, name: String) -> Result<(), String> {
        let mut st = self.inner.lock().unwrap();
        st.ensure_idle()?;
        if st.pending.is_some() {
            return Err("请先处理未保存的录制".into());
        }
        let name = checked_name(name)?;
        let doc = TaskDocument::new(
            name,
            platform::snapshot_geometry(),
            platform::current_layout_klid(),
        );
        st.store.insert(&doc)?;
        st.current = doc;
        st.demo = None;
        st.last_error = None;
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn select_task(&self, app: &AppHandle, id: &str) -> Result<(), String> {
        let mut st = self.inner.lock().unwrap();
        st.ensure_idle()?;
        if st.pending.is_some() {
            return Err("请先处理未保存的录制".into());
        }
        let next = st.store.load(id)?;
        st.store.set_setting("activeTask", id)?;
        st.current = next;
        st.demo = None;
        st.last_error = None;
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn rename_task(
        &self,
        app: &AppHandle,
        id: &str,
        revision: u64,
        name: String,
    ) -> Result<(), String> {
        let mut st = self.inner.lock().unwrap();
        st.ensure_idle()?;
        st.check(id, revision)?;
        if st.demo.is_some() {
            return Err("演示任务不能重命名".into());
        }
        let mut next = st.current.clone();
        next.name = checked_name(name)?;
        st.commit(next)?;
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn delete_task(&self, app: &AppHandle, id: &str, revision: u64) -> Result<(), String> {
        let mut st = self.inner.lock().unwrap();
        st.ensure_idle()?;
        st.check(id, revision)?;
        if st.demo.is_some() || st.pending.is_some() {
            return Err("请先退出演示或处理未保存录制".into());
        }
        let next_id = st
            .store
            .list()?
            .into_iter()
            .find(|t| t.id != id)
            .map(|t| t.id);
        let fallback = if next_id.is_none() {
            Some(TaskDocument::new(
                "我的任务".into(),
                platform::snapshot_geometry(),
                platform::current_layout_klid(),
            ))
        } else {
            None
        };
        let next = if let Some(doc) = &fallback {
            doc.clone()
        } else {
            st.store.load(next_id.as_ref().unwrap())?
        };
        st.store
            .delete_select(id, revision, &next.id, fallback.as_ref())?;
        st.histories.remove(id);
        st.current = next;
        st.last_error = None;
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn edit(
        &self,
        app: &AppHandle,
        id: &str,
        revision: u64,
        edit: EditRequest,
    ) -> Result<(), String> {
        let mut st = self.inner.lock().unwrap();
        st.ensure_idle()?;
        st.check(id, revision)?;
        let mut next = st.doc().clone();
        next.apply(edit)?;
        st.commit(next)?;
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn history(
        &self,
        app: &AppHandle,
        id: &str,
        revision: u64,
        redo: bool,
    ) -> Result<(), String> {
        let mut st = self.inner.lock().unwrap();
        st.ensure_idle()?;
        st.check(id, revision)?;
        let history = st.histories.entry(id.into()).or_default();
        let mut next = if redo {
            history.future.last()
        } else {
            history.past.last()
        }
        .cloned()
        .ok_or("没有可撤销或重做的修改")?;
        let old = st.doc().clone();
        next.revision = old.revision.checked_add(1).ok_or("修订号超出范围")?;
        next.updated_at = wall_ms();
        if st.demo.is_none() {
            st.store.save(&next, old.revision)?;
        }
        let history = st.histories.get_mut(id).unwrap();
        if redo {
            history.future.pop();
            history.past.push(old);
        } else {
            history.past.pop();
            history.future.push(old);
        }
        if st.demo.is_some() {
            st.demo = Some(next);
        } else {
            st.current = next;
        }
        st.last_error = None;
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn set_hotkeys(&self, app: &AppHandle, hotkeys: Hotkeys) -> Result<(), String> {
        hotkeys.validate()?;
        let mut st = self.inner.lock().unwrap();
        st.ensure_idle()?;
        self.input_service()?.set_hotkeys(hotkeys.clone())?;
        let json = serde_json::to_string(&hotkeys).map_err(|_| "快捷键配置无效")?;
        if let Err(reason) = st.store.set_setting("hotkeys", &json) {
            let _ = self.input_service()?.set_hotkeys(st.hotkeys.clone());
            return Err(reason);
        }
        st.hotkeys = hotkeys;
        st.last_error = None;
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn demo(&self, app: &AppHandle, enter: bool) -> Result<(), String> {
        let mut st = self.inner.lock().unwrap();
        st.ensure_idle()?;
        if enter {
            if st.pending.is_some() {
                return Err("请先处理未保存录制".into());
            }
            st.demo = Some(demo_document());
        } else {
            if let Some(doc) = st.demo.take() {
                st.histories.remove(&doc.id);
            }
        }
        drop(st);
        self.emit_state(app);
        Ok(())
    }
    pub fn start_execution(
        self: &Arc<Self>,
        app: &AppHandle,
        id: &str,
        revision: u64,
        rounds: u32,
        speed: f64,
    ) -> Result<(), String> {
        if !(1..=100_000).contains(&rounds) && rounds != INFINITE_ROUNDS {
            return Err("执行次数应为 1–100000 的整数，或无限循环".into());
        }
        let quarters = speed * 4.0;
        if !quarters.is_finite() || !(1.0..=8.0).contains(&quarters) || quarters.fract() != 0.0 {
            return Err("执行速度应为 0.25x 至 2x，每次调整 0.25x".into());
        }
        let previous = self.executor.lock().unwrap().take();
        if let Some(handle) = previous {
            if handle.is_finished() {
                let _ = handle.join();
            } else {
                *self.executor.lock().unwrap() = Some(handle);
                return Err("上次执行还在收尾".into());
            }
        }
        let (spec, stop, progress, mut driver, prepare_until) = {
            let mut st = self.inner.lock().unwrap();
            st.ensure_idle()?;
            st.check(id, revision)?;
            if st.demo.is_some() {
                return Err("演示只支持预览，不能执行到系统".into());
            }
            self.input_service()?.ensure_stop_available()?;
            let mut events = st.current.compile_for_execution()?;
            let scale_time = |time: i64| scale_execution_time(time, quarters as u8);
            for event in &mut events {
                event.time_us = scale_time(event.time_us)?;
            }
            let geometry = platform::snapshot_geometry();
            if !geometry.same_layout(&st.current.geometry) {
                return Err("显示器布局与录制时不一致，请恢复原布局后执行".into());
            }
            let spec = ScheduleSpec {
                events: Arc::new(events),
                duration_us: scale_time(st.current.duration_us)?,
                rounds,
            };
            let stop = StopSignal::new();
            let driver = platform::make_driver(&stop, &geometry)?;
            *self.execution_stop.lock().unwrap() = Some(stop.clone());
            if let Err(error) = self.input_service()?.arm_execution(true) {
                *self.execution_stop.lock().unwrap() = None;
                return Err(error);
            }
            if let Err(error) = crate::overlay::show_status(app) {
                let _ = self.input_service()?.arm_execution(false);
                *self.execution_stop.lock().unwrap() = None;
                return Err(error);
            }
            let progress = Arc::new(ExecutionProgress::default());
            let prepare_until = Instant::now() + Duration::from_secs(3);
            st.mode = Mode::Executing(ExecutionLive {
                stop: stop.clone(),
                progress: progress.clone(),
                rounds,
                speed,
                prepare_until,
            });
            st.last_error = None;
            (spec, stop, progress, driver, prepare_until)
        };
        let session = self.clone();
        let app_worker = app.clone();
        let worker = std::thread::Builder::new()
            .name("repeat-executor".into())
            .spawn(move || {
                #[cfg(windows)]
                let click_highlight =
                    crate::platform::windows::click_highlight::ClickHighlight::start();
                let outcome = match driver.wait_until(prepare_until) {
                    PacerOutcome::Cancelled => empty_outcome(rounds, true, stop.failure_reason()),
                    PacerOutcome::Failed(reason) => empty_outcome(rounds, false, Some(reason)),
                    PacerOutcome::Deadline if stop.is_signalled() => {
                        empty_outcome(rounds, true, None)
                    }
                    PacerOutcome::Deadline => {
                        if let Err(reason) = platform::execution_ready() {
                            empty_outcome(rounds, false, Some(reason))
                        } else if let Err(reason) =
                            crate::overlay::show_execution_borders(&app_worker)
                        {
                            empty_outcome(rounds, false, Some(reason))
                        } else {
                            progress.phase.store(1, Ordering::SeqCst);
                            session.emit_state(&app_worker);
                            let mut last = Instant::now()
                                .checked_sub(Duration::from_millis(100))
                                .unwrap_or_else(Instant::now);
                            run_replay(&spec, &mut driver, &stop, LATE_THRESHOLD, |event| {
                                match event {
                                    ProgressEvent::RoundStart { round } => {
                                        progress.round.store(round, Ordering::SeqCst);
                                        *progress.round_started.lock().unwrap() =
                                            Some(Instant::now());
                                    }
                                    ProgressEvent::Injected {
                                        round,
                                        seq,
                                        kind: _kind,
                                    } => {
                                        progress.round.store(round, Ordering::SeqCst);
                                        progress.last_seq.store(seq, Ordering::SeqCst);
                                        progress.injected.fetch_add(1, Ordering::SeqCst);
                                        #[cfg(windows)]
                                        if let crate::event::EventKind::MouseDown { x, y, .. } =
                                            _kind
                                            && let Some(highlight) = &click_highlight
                                        {
                                            highlight.show(x, y);
                                        }
                                    }
                                    ProgressEvent::RoundComplete { .. } => {}
                                }
                                if last.elapsed() >= Duration::from_millis(100) {
                                    last = Instant::now();
                                    session.emit_state(&app_worker);
                                }
                            })
                        }
                    }
                };
                #[cfg(windows)]
                drop(click_highlight);
                if let Some(service) = session.service.get() {
                    let _ = service.arm_execution(false);
                }
                crate::overlay::hide(&app_worker);
                let stats = compute_stats(&outcome.timing);
                let mut st = session.inner.lock().unwrap();
                if let Some(failure) = &outcome.failure {
                    st.last_error = Some(format!(
                        "第 {} 轮执行失败：{}",
                        failure.round, failure.reason
                    ));
                }
                st.report = Some(ExecutionReportBundle {
                    outcome,
                    stats,
                    planned_rounds: rounds,
                });
                st.mode = Mode::Idle;
                *session.execution_stop.lock().unwrap() = None;
                drop(st);
                session.emit_state(&app_worker);
            });
        match worker {
            Ok(handle) => *self.executor.lock().unwrap() = Some(handle),
            Err(error) => {
                self.inner.lock().unwrap().mode = Mode::Idle;
                *self.execution_stop.lock().unwrap() = None;
                let _ = self.input_service()?.arm_execution(false);
                crate::overlay::hide(app);
                self.emit_state(app);
                return Err(format!("无法启动执行：{error}"));
            }
        }
        self.emit_state(app);
        Ok(())
    }
    pub fn stop_execution(&self, app: &AppHandle) -> Result<(), String> {
        {
            let st = self.inner.lock().unwrap();
            let Mode::Executing(live) = &st.mode else {
                return Err("当前没有系统执行".into());
            };
            live.stop.signal();
        }
        self.emit_state(app);
        Ok(())
    }
    fn snapshot_locked(&self, st: &SessionState) -> SessionSnapshot {
        let (mode, recording_live, execution_live) = match &st.mode {
            Mode::Idle => ("idle", None, None),
            Mode::Recording(live) => {
                let buffer = live.buffer.lock().unwrap();
                (
                    "recording",
                    Some(RecordingLiveView {
                        elapsed_us: buffer.elapsed_us(),
                        event_count: buffer.event_count(),
                    }),
                    None,
                )
            }
            Mode::Executing(live) => (
                if live.progress.phase.load(Ordering::SeqCst) == 0 {
                    "preparing"
                } else {
                    "executing"
                },
                None,
                Some(ExecutionLiveView {
                    rounds_requested: live.rounds,
                    speed: live.speed,
                    current_round: live.progress.round.load(Ordering::SeqCst),
                    events_injected: live.progress.injected.load(Ordering::SeqCst),
                    last_seq: live.progress.last_seq.load(Ordering::SeqCst),
                    stop_requested: live.stop.is_signalled(),
                    elapsed_us: live
                        .progress
                        .round_started
                        .lock()
                        .unwrap()
                        .map(|t| {
                            ((t.elapsed().as_micros() as f64 * live.speed)
                                .min(st.current.duration_us.max(0) as f64))
                                as i64
                        })
                        .unwrap_or(0),
                    prepare_remaining_ms: live
                        .prepare_until
                        .saturating_duration_since(Instant::now())
                        .as_millis() as u64,
                }),
            ),
        };
        let history = st.histories.get(&st.doc().id);
        SessionSnapshot {
            version: self.next_snapshot.fetch_add(1, Ordering::SeqCst),
            mode,
            recording_live,
            execution_live,
            task_id: st.doc().id.clone(),
            revision: st.doc().revision,
            has_recording: !st.doc().actions.is_empty(),
            has_execution_report: st.report.is_some(),
            last_error: st.last_error.clone(),
            hotkey_status: self
                .service
                .get()
                .map(|s| s.status_lines())
                .unwrap_or_default(),
            hotkeys: st.hotkeys.clone(),
            can_undo: history.is_some_and(|h| !h.past.is_empty()),
            can_redo: history.is_some_and(|h| !h.future.is_empty()),
            is_demo: st.demo.is_some(),
            has_pending_recording: st.pending.is_some(),
        }
    }
    pub fn snapshot(&self) -> SessionSnapshot {
        self.snapshot_locked(&self.inner.lock().unwrap())
    }
    pub fn workspace(&self) -> Result<WorkspaceView, String> {
        let st = self.inner.lock().unwrap();
        Ok(WorkspaceView {
            session: self.snapshot_locked(&st),
            document: st.doc().view(),
            tasks: st.store.list()?,
            data_path: self.data_path.clone(),
        })
    }
    pub fn report(&self) -> Option<ExecutionReportBundle> {
        self.inner.lock().unwrap().report.clone()
    }
    pub fn emit_state(&self, app: &AppHandle) {
        let _ = app.emit("state-changed", self.snapshot());
    }
}
fn checked_name(name: String) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 100 {
        Err("任务名称应为 1–100 个字符".into())
    } else {
        Ok(name.into())
    }
}
fn empty_outcome(rounds: u32, stopped: bool, error: Option<String>) -> ExecutionOutcome {
    ExecutionOutcome {
        rounds_requested: rounds,
        rounds_completed: 0,
        events_injected: 0,
        stopped,
        stop_observed_after_us: None,
        failure: error.map(|reason| ExecutionFailure {
            round: 1,
            seq: None,
            reason,
        }),
        timing: vec![],
        released: vec![],
        late_threshold_ms: LATE_THRESHOLD.as_millis() as u64,
    }
}
fn demo_document() -> TaskDocument {
    use crate::event::RecordedEvent;
    use crate::geometry::{DesktopGeometry, MonitorInfo};
    let geometry = DesktopGeometry {
        monitors: vec![MonitorInfo {
            name: "演示桌面".into(),
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
            dpi: 96,
        }],
    };
    let mut doc = TaskDocument::new("使用演示".into(), geometry.clone(), "demo".into());
    let mut events: Vec<RecordedEvent> = Vec::new();
    let mut push = |time_us: i64, kind: EventKind| {
        let seq = events.len() as u64 + 1;
        events.push(RecordedEvent {
            seq,
            time_us,
            native_time_ms: (time_us / 1000) as u32,
            kind,
        });
    };
    let move_a = |push: &mut dyn FnMut(i64, EventKind)| {
        for i in 0..=12 {
            let f = i as f64 / 12.0;
            let time_us = 400_000 + (600_000.0 * f) as i64;
            push(
                time_us,
                EventKind::MouseMove {
                    x: (160.0 + (430.0 - 160.0) * f) as i32,
                    y: (190.0 + (280.0 - 190.0) * f) as i32,
                },
            );
        }
    };
    move_a(&mut push);
    push(
        1_120_000,
        EventKind::MouseDown {
            button: MouseButton::Left,
            x: 430,
            y: 280,
        },
    );
    push(
        1_200_000,
        EventKind::MouseUp {
            button: MouseButton::Left,
            x: 430,
            y: 280,
        },
    );
    push(
        1_750_000,
        EventKind::KeyDown {
            vk: 0x42,
            scan: 0,
            extended: false,
        },
    );
    push(
        1_850_000,
        EventKind::KeyUp {
            vk: 0x42,
            scan: 0,
            extended: false,
        },
    );
    push(
        2_550_000,
        EventKind::KeyDown {
            vk: 0x09,
            scan: 0,
            extended: false,
        },
    );
    push(
        2_670_000,
        EventKind::KeyUp {
            vk: 0x09,
            scan: 0,
            extended: false,
        },
    );
    push(
        3_900_000,
        EventKind::KeyDown {
            vk: 0x11,
            scan: 0,
            extended: false,
        },
    );
    push(
        3_910_000,
        EventKind::KeyDown {
            vk: 0x53,
            scan: 0,
            extended: false,
        },
    );
    push(
        4_000_000,
        EventKind::KeyUp {
            vk: 0x53,
            scan: 0,
            extended: false,
        },
    );
    push(
        4_020_000,
        EventKind::KeyUp {
            vk: 0x11,
            scan: 0,
            extended: false,
        },
    );
    for i in 0..=12 {
        let f = i as f64 / 12.0;
        let time_us = 4_550_000 + (1_150_000.0 * f) as i64;
        push(
            time_us,
            EventKind::MouseMove {
                x: (430.0 + (940.0 - 430.0) * f) as i32,
                y: (280.0 + (650.0 - 280.0) * f) as i32,
            },
        );
    }
    push(
        5_900_000,
        EventKind::MouseDown {
            button: MouseButton::Left,
            x: 940,
            y: 650,
        },
    );
    push(
        5_980_000,
        EventKind::MouseUp {
            button: MouseButton::Left,
            x: 940,
            y: 650,
        },
    );
    push(
        6_700_000,
        EventKind::KeyDown {
            vk: 0x0d,
            scan: 0,
            extended: false,
        },
    );
    push(
        6_800_000,
        EventKind::KeyUp {
            vk: 0x0d,
            scan: 0,
            extended: false,
        },
    );
    doc.replace_recording(RawRecording {
        gestures: Default::default(),
        events,
        duration_us: 6_800_000,
        geometry,
        layout_klid: "demo".into(),
        started_at_wall_ms: 0,
    });
    doc
}
