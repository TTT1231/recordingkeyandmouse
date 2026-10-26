//! 回放调度：以绝对截止时间驱动事件注入（ADR-0001 第 6 节）。
//!
//! 核心规则：
//! - 计划截止时间 = 本轮单调起点 + 事件相对时间；每次等待都对准绝对截止时间，
//!   不做"睡间隔→执行→再睡下一个间隔"的累积漂移。
//! - 循环执行按定稿：轮间不额外插等待，但每轮完整覆盖录制的结尾停顿（原始停顿属于录制内容）；
//!   任一轮失败或停止即结束，报告轮次。
//! - 注入严重落后（内部阈值 1000ms 初值）即停止并报告，不补发过期输入。
//! - 停止/失败/完成后仅释放本次由引擎注入且仍按下的输入（按键收尾）。
//!
//! 调度器通过 `ReplayDriver` 抽象时钟、等待与注入：生产环境接 Windows 高精度
//! 可等待计时器与 SendInput；测试接确定性模拟驱动。模拟测试不替代原生验证。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::event::EventKind;

/// 注入严重落后的判定阈值（内部自检参数，2026-10-26 定稿初值 1000ms，按实测调整）。
pub const LATE_THRESHOLD: Duration = Duration::from_millis(1000);

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PacerOutcome {
    Deadline,
    Cancelled,
    Failed(String),
}

/// 调度依赖的时钟、等待与注入抽象。
pub trait ReplayDriver {
    fn now(&mut self) -> Instant;
    /// 等待到截止时间；被停止信号打断时返回 `Cancelled`。
    fn wait_until(&mut self, deadline: Instant) -> PacerOutcome;
    /// 注入单个事件。失败返回可展示的原因（如 SendInput 部分成功）。
    fn inject(&mut self, kind: &EventKind) -> Result<(), String>;
    /// 释放本次执行中由引擎注入且仍按下的输入，返回释放描述。
    fn release_held(&mut self) -> Result<Vec<String>, String>;
}

/// 原生停止信号：布尔标志 + 发出时刻 + 可选的线程唤醒回调（Windows 事件句柄 SetEvent）。
/// 停止通路不经过前端；`signal` 可从任意线程调用。
pub struct StopSignal {
    flag: AtomicBool,
    failure_reason: Mutex<Option<String>>,
    signalled_at: Mutex<Option<Instant>>,
    waker: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl StopSignal {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            flag: AtomicBool::new(false),
            failure_reason: Mutex::new(None),
            signalled_at: Mutex::new(None),
            waker: Mutex::new(None),
        })
    }

    pub fn signal(&self) {
        let mut at = self.signalled_at.lock().unwrap();
        if !self.flag.swap(true, Ordering::SeqCst) {
            *at = Some(Instant::now());
            drop(at);
            if let Some(w) = self.waker.lock().unwrap().clone() {
                w();
            }
        }
    }

    pub fn fail(&self, reason: &str) {
        *self.failure_reason.lock().unwrap() = Some(reason.into());
        self.signal();
    }
    pub fn failure_reason(&self) -> Option<String> {
        self.failure_reason.lock().unwrap().clone()
    }

    pub fn is_signalled(&self) -> bool {
        self.flag.load(Ordering::SeqCst)
    }

    pub fn signalled_at(&self) -> Option<Instant> {
        *self.signalled_at.lock().unwrap()
    }

    /// 注册唤醒回调（原生驱动把等待线程叫醒用）；测试环境缺省为空。
    pub fn set_waker(&self, waker: Arc<dyn Fn() + Send + Sync>) {
        *self.waker.lock().unwrap() = Some(waker.clone());
        if self.is_signalled() {
            waker();
        }
    }

    /// 测试辅助：以给定时刻记为停止发出时间。
    #[cfg(test)]
    pub fn signal_at_for_test(&self, at: Instant) {
        if !self.flag.swap(true, Ordering::SeqCst) {
            *self.signalled_at.lock().unwrap() = Some(at);
        }
    }
}

/// 一次执行的计划输入。
pub struct ScheduleSpec {
    pub events: Arc<Vec<crate::event::RecordedEvent>>,
    /// 录制起止时长（含结尾停顿）。每轮回放完整覆盖该时长。
    pub duration_us: i64,
    pub rounds: u32,
}

/// 执行前数据校验：时间非负、非递减、序号严格递增。数据损坏必须拒绝执行而不是猜。
pub fn validate_recording(
    events: &[crate::event::RecordedEvent],
    duration_us: i64,
) -> Result<(), String> {
    if duration_us < 0 {
        return Err("录制时长为负，数据异常".into());
    }
    let mut last_time = 0i64;
    let mut last_seq = 0u64;
    for e in events {
        if e.time_us < 0 {
            return Err(format!("事件 {} 时间为负", e.seq));
        }
        if e.time_us < last_time {
            return Err(format!("事件 {} 时间早于前一事件，顺序损坏", e.seq));
        }
        last_time = e.time_us;
        if e.seq <= last_seq {
            return Err(format!("事件序号不递增（{} 之后出现 {}）", last_seq, e.seq));
        }
        last_seq = e.seq;
        if e.time_us > duration_us {
            return Err(format!("事件 {} 晚于录制时长，数据异常", e.seq));
        }
    }
    Ok(())
}

/// 进度回调数据。
#[derive(Clone, Copy, Debug)]
pub enum ProgressEvent {
    RoundStart { round: u32 },
    Injected { round: u32, seq: u64 },
    RoundComplete { round: u32 },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimingRecord {
    pub round: u32,
    pub seq: u64,
    pub planned_us: i64,
    pub actual_us: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionFailure {
    pub round: u32,
    pub seq: Option<u64>,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionOutcome {
    pub rounds_requested: u32,
    pub rounds_completed: u32,
    pub events_injected: u64,
    /// 用户停止（按钮 / F9）导致结束。
    pub stopped: bool,
    /// 停止信号发出到调度线程确认取消的时间（微秒）；确认后不应再有任何新注入。
    pub stop_observed_after_us: Option<i64>,
    pub failure: Option<ExecutionFailure>,
    pub timing: Vec<TimingRecord>,
    /// 收尾释放的输入描述（仅引擎本次注入且仍按下的）。
    pub released: Vec<String>,
    pub late_threshold_ms: u64,
}

/// 执行统计（M0-03 测量字段）。偏差绝对值的分位数、首轮首末误差与累计漂移。
#[derive(Clone, Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TimingStats {
    pub sample_count: u64,
    pub p50_abs_us: i64,
    pub p95_abs_us: i64,
    pub p99_abs_us: i64,
    pub max_abs_us: i64,
    pub first_round_first_err_us: i64,
    pub first_round_last_err_us: i64,
    /// 累计漂移 = 首轮末次误差 - 首轮首次误差（内部目标绝对值 ≤ 20ms）。
    pub drift_us: i64,
}

pub fn compute_stats(timing: &[TimingRecord]) -> TimingStats {
    let mut stats = TimingStats {
        sample_count: timing.len() as u64,
        ..Default::default()
    };
    if timing.is_empty() {
        return stats;
    }
    let mut abs_errs: Vec<i64> = timing
        .iter()
        .map(|t| (t.actual_us - t.planned_us).abs())
        .collect();
    abs_errs.sort_unstable();
    let pick = |q: f64| -> i64 {
        let n = abs_errs.len();
        let idx = ((q * n as f64).ceil() as usize).clamp(1, n) - 1;
        abs_errs[idx]
    };
    stats.p50_abs_us = pick(0.50);
    stats.p95_abs_us = pick(0.95);
    stats.p99_abs_us = pick(0.99);
    stats.max_abs_us = *abs_errs.last().unwrap();
    // 用排序键找出时间上的首末：取 planned 最小/最大，同值取序号小的（Reverse 后为大序号）。
    // 首末误差只看第 1 轮（轮与轮之间会重新对表，跨轮混合没有意义）。
    let round1: Vec<&TimingRecord> = timing.iter().filter(|t| t.round == 1).collect();
    if let Some(first) = round1.iter().min_by_key(|t| t.time_key()) {
        stats.first_round_first_err_us = first.actual_us - first.planned_us;
    }
    if let Some(last) = round1.iter().max_by_key(|t| t.time_key()) {
        stats.first_round_last_err_us = last.actual_us - last.planned_us;
    }
    stats.drift_us = stats.first_round_last_err_us - stats.first_round_first_err_us;
    stats
}

impl TimingRecord {
    fn time_key(&self) -> (i64, u64) {
        (self.planned_us, self.seq)
    }
}

fn micros(us: i64) -> Duration {
    Duration::from_micros(us.max(0) as u64)
}

/// 执行一轮内的所有事件，并在轮末等待剩余的结尾停顿。
/// 返回值表示本轮是否完整结束（false = 停止或失败）。
fn run_round(
    round: u32,
    spec: &ScheduleSpec,
    driver: &mut dyn ReplayDriver,
    stop: &StopSignal,
    late_threshold: Duration,
    timing: &mut Vec<TimingRecord>,
    on_progress: &mut dyn FnMut(ProgressEvent),
) -> RoundEnd {
    let round_start = driver.now();
    let last_event_us = spec.events.last().map(|e| e.time_us).unwrap_or(0);
    let round_len_us = spec.duration_us.max(last_event_us);

    for ev in spec.events.iter() {
        if stop.is_signalled() {
            return RoundEnd::Stopped;
        }
        let deadline = round_start + micros(ev.time_us);
        match driver.wait_until(deadline) {
            PacerOutcome::Cancelled => return RoundEnd::Stopped,
            PacerOutcome::Failed(reason) => {
                return RoundEnd::Failed(ExecutionFailure {
                    round,
                    seq: Some(ev.seq),
                    reason,
                });
            }
            PacerOutcome::Deadline => {}
        }
        if stop.is_signalled() {
            return RoundEnd::Stopped;
        }
        let before_us = driver
            .now()
            .saturating_duration_since(round_start)
            .as_micros()
            .min(i64::MAX as u128) as i64;
        if before_us.saturating_sub(ev.time_us) > late_threshold.as_micros() as i64 {
            return RoundEnd::Failed(ExecutionFailure {
                round,
                seq: Some(ev.seq),
                reason: format!(
                    "落后计划超过 {}ms，已停止且未发送过期输入",
                    late_threshold.as_millis()
                ),
            });
        }
        let inject_result = driver.inject(&ev.kind);
        let actual_us = (driver.now() - round_start)
            .as_micros()
            .min(i64::MAX as u128) as i64;
        match inject_result {
            Err(reason) => {
                return RoundEnd::Failed(ExecutionFailure {
                    round,
                    seq: Some(ev.seq),
                    reason,
                });
            }
            Ok(()) => {
                timing.push(TimingRecord {
                    round,
                    seq: ev.seq,
                    planned_us: ev.time_us,
                    actual_us,
                });
                on_progress(ProgressEvent::Injected { round, seq: ev.seq });
            }
        }
        if actual_us.saturating_sub(ev.time_us) > late_threshold.as_micros() as i64 {
            return RoundEnd::Failed(ExecutionFailure {
                round,
                seq: Some(ev.seq),
                reason: format!(
                    "注入落后计划超过 {}ms，按约定停止且不补发过期输入",
                    late_threshold.as_millis()
                ),
            });
        }
    }

    // 结尾停顿属于录制内容：每轮完整等待到录制时长，轮与轮之间不额外加等待。
    let round_end = round_start + micros(round_len_us);
    match driver.wait_until(round_end) {
        PacerOutcome::Cancelled => return RoundEnd::Stopped,
        PacerOutcome::Failed(reason) => {
            return RoundEnd::Failed(ExecutionFailure {
                round,
                seq: None,
                reason,
            });
        }
        PacerOutcome::Deadline => {}
    }
    if stop.is_signalled() {
        return RoundEnd::Stopped;
    }
    RoundEnd::Complete
}

enum RoundEnd {
    Complete,
    Stopped,
    Failed(ExecutionFailure),
}

/// 运行整个执行（可能多轮）。调用方负责在结束后驱动 `release_held` 之外的收尾。
pub fn run_replay(
    spec: &ScheduleSpec,
    driver: &mut dyn ReplayDriver,
    stop: &StopSignal,
    late_threshold: Duration,
    mut on_progress: impl FnMut(ProgressEvent),
) -> ExecutionOutcome {
    let mut timing: Vec<TimingRecord> = Vec::new();
    let mut stopped = false;
    let mut failure: Option<ExecutionFailure> = None;
    let mut rounds_completed = 0u32;

    for round in 1..=spec.rounds {
        on_progress(ProgressEvent::RoundStart { round });
        match run_round(
            round,
            spec,
            driver,
            stop,
            late_threshold,
            &mut timing,
            &mut on_progress,
        ) {
            RoundEnd::Complete => {
                on_progress(ProgressEvent::RoundComplete { round });
                rounds_completed = round;
            }
            RoundEnd::Stopped => {
                stopped = true;
                break;
            }
            RoundEnd::Failed(f) => {
                failure = Some(f);
                break;
            }
        }
    }

    if failure.is_none()
        && let Some(reason) = stop.failure_reason()
    {
        failure = Some(ExecutionFailure {
            round: (rounds_completed + 1).min(spec.rounds),
            seq: timing.last().map(|t| t.seq),
            reason,
        });
    }
    let stop_observed_after_us = stop
        .signalled_at()
        .map(|at| (driver.now().saturating_duration_since(at)).as_micros() as i64);

    let released = match driver.release_held() {
        Ok(released) => released,
        Err(reason) => {
            if let Some(f) = &mut failure {
                f.reason.push_str(&format!("；收尾失败：{reason}"));
            } else {
                failure = Some(ExecutionFailure {
                    round: (rounds_completed + 1).min(spec.rounds),
                    seq: None,
                    reason: format!("收尾释放失败：{reason}"),
                });
            }
            Vec::new()
        }
    };

    ExecutionOutcome {
        rounds_requested: spec.rounds,
        rounds_completed,
        events_injected: timing.len() as u64,
        stopped,
        stop_observed_after_us,
        failure,
        timing,
        released,
        late_threshold_ms: late_threshold.as_millis() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{MouseButton, RecordedEvent};

    /// 确定性模拟驱动：时间只在 wait_until 前进；可注入滞后、取消时刻与失败序列号。
    struct MockDriver {
        now: Instant,
        stop: Arc<StopSignal>,
        /// 每次等待额外滞后的时长。
        lag: Duration,
        /// 模拟时间轴上的取消时刻（一旦越过即视为已停止）。
        cancel_at: Option<Instant>,
        /// 指定序号注入失败。
        fail_on_seq: Option<u64>,
        injected: Vec<(u32, u64, i64)>, // (round 由调用方跟踪不便，这里记 seq 与 actual)
        held: Vec<EventKind>,
        released_desc: Vec<String>,
        stop_on_deadline: bool,
        wait_failure: bool,
        cleanup_failure: bool,
    }

    impl MockDriver {
        fn new(stop: Arc<StopSignal>) -> Self {
            Self {
                now: Instant::now(),
                stop,
                lag: Duration::ZERO,
                cancel_at: None,
                fail_on_seq: None,
                injected: Vec::new(),
                held: Vec::new(),
                released_desc: Vec::new(),
                stop_on_deadline: false,
                wait_failure: false,
                cleanup_failure: false,
            }
        }
    }

    impl ReplayDriver for MockDriver {
        fn now(&mut self) -> Instant {
            self.now
        }
        fn wait_until(&mut self, deadline: Instant) -> PacerOutcome {
            if self.wait_failure {
                return PacerOutcome::Failed("模拟等待失败".into());
            }
            if self.stop_on_deadline {
                self.stop.signal();
                return PacerOutcome::Deadline;
            }
            if self.stop.is_signalled() {
                return PacerOutcome::Cancelled;
            }
            // 已到期的截止时间：与真实驱动一致，立即返回且不计滞后。
            if deadline <= self.now {
                return PacerOutcome::Deadline;
            }
            let wake = if let Some(ct) = self.cancel_at {
                if self.now < ct && ct < deadline {
                    ct
                } else {
                    deadline
                }
            } else {
                deadline
            };
            self.now = wake + self.lag;
            if Some(wake) == self.cancel_at && !self.stop.is_signalled() {
                self.stop.signal_at_for_test(wake);
                return PacerOutcome::Cancelled;
            }
            PacerOutcome::Deadline
        }
        fn inject(&mut self, kind: &EventKind) -> Result<(), String> {
            // 记录"实际注入时刻"相对 round_start 由测试侧换算；这里保存序号+事件。
            let seq = self.injected.len() as u64 + 1;
            if self.fail_on_seq == Some(seq) {
                return Err("模拟注入失败".into());
            }
            if kind.is_key_down() || matches!(kind, EventKind::MouseDown { .. }) {
                self.held.push(*kind);
            }
            if kind.is_key_up() || matches!(kind, EventKind::MouseUp { .. }) {
                self.held.pop();
            }
            self.injected.push((0, seq, 0));
            Ok(())
        }
        fn release_held(&mut self) -> Result<Vec<String>, String> {
            if self.cleanup_failure {
                return Err("模拟释放失败".into());
            }
            self.released_desc = self.held.iter().map(|k| k.describe()).collect();
            Ok(std::mem::take(&mut self.released_desc))
        }
    }

    fn ev(seq: u64, time_us: i64, kind: EventKind) -> RecordedEvent {
        RecordedEvent {
            seq,
            time_us,
            native_time_ms: 0,
            kind,
        }
    }

    fn key(vk: u32, down: bool) -> EventKind {
        if down {
            EventKind::KeyDown {
                vk,
                scan: 0,
                extended: false,
            }
        } else {
            EventKind::KeyUp {
                vk,
                scan: 0,
                extended: false,
            }
        }
    }

    fn spec_of(events: Vec<RecordedEvent>, duration_us: i64, rounds: u32) -> ScheduleSpec {
        ScheduleSpec {
            events: Arc::new(events),
            duration_us,
            rounds,
        }
    }

    #[test]
    fn ideal_clock_has_zero_drift_and_keeps_round_rhythm() {
        // 事件 0ms / 1000ms / 2500ms；时长 3000ms（含 500ms 结尾停顿）；2 轮。
        let events = vec![
            ev(1, 0, EventKind::MouseMove { x: 1, y: 1 }),
            ev(
                2,
                1_000_000,
                EventKind::MouseDown {
                    button: MouseButton::Left,
                    x: 200,
                    y: 150,
                },
            ),
            ev(
                3,
                1_000_000,
                EventKind::MouseUp {
                    button: MouseButton::Left,
                    x: 200,
                    y: 150,
                },
            ),
            ev(4, 2_500_000, key(0x41, true)),
            ev(5, 2_500_000, key(0x41, false)),
        ];
        let spec = spec_of(events, 3_000_000, 2);
        let stop = StopSignal::new();
        let mut driver = MockDriver::new(stop.clone());
        let outcome = run_replay(&spec, &mut driver, &stop, LATE_THRESHOLD, |_| {});
        assert_eq!(outcome.rounds_completed, 2);
        assert_eq!(outcome.events_injected, 10);
        assert!(outcome.failure.is_none());
        assert!(!outcome.stopped);
        // 理想时钟：实际注入时刻 == 计划时刻，零漂移。
        for t in &outcome.timing {
            assert_eq!(t.actual_us, t.planned_us, "理想时钟不应有偏差: {t:?}");
        }
        // 第二轮起点 = 第一轮起点 + 时长（结尾停顿被完整等待，轮间无额外等待）。
        let round2_first = outcome.timing.iter().find(|t| t.round == 2).unwrap();
        assert_eq!(round2_first.planned_us, 0);
        // 同时间戳事件保持序号顺序。
        let same_time: Vec<u64> = outcome
            .timing
            .iter()
            .filter(|t| t.round == 1 && t.planned_us == 1_000_000)
            .map(|t| t.seq)
            .collect();
        assert_eq!(same_time, vec![2, 3]);
        let stats = compute_stats(&outcome.timing);
        assert_eq!(stats.drift_us, 0);
        assert_eq!(stats.max_abs_us, 0);
    }

    #[test]
    fn stop_during_wait_releases_held_keys() {
        // 事件：0ms 按 A 不放，2000ms 抬起；取消发生在 1200ms。
        let events = vec![
            ev(1, 0, key(0x41, true)),
            ev(2, 2_000_000, key(0x41, false)),
        ];
        let spec = spec_of(events, 2_000_000, 1);
        let stop = StopSignal::new();
        let mut driver = MockDriver::new(stop.clone());
        driver.cancel_at = Some(driver.now + Duration::from_millis(1200));
        let outcome = run_replay(&spec, &mut driver, &stop, LATE_THRESHOLD, |_| {});
        assert!(outcome.stopped);
        assert_eq!(outcome.rounds_completed, 0);
        assert_eq!(outcome.events_injected, 1, "只注入了第一个事件");
        assert_eq!(outcome.released.len(), 1, "未抬起的按键必须收尾释放");
        assert!(outcome.stop_observed_after_us.is_some());
        // 停止确认后不能再有新注入：timing 里只有取消前的事件。
        assert!(outcome.timing.iter().all(|t| t.seq <= 1));
    }

    #[test]
    fn inject_failure_reports_round_and_seq() {
        let events = vec![
            ev(1, 0, key(0x41, true)),
            ev(2, 500_000, key(0x41, false)),
            ev(3, 900_000, EventKind::MouseMove { x: 9, y: 9 }),
        ];
        let spec = spec_of(events, 1_000_000, 3);
        let stop = StopSignal::new();
        let mut driver = MockDriver::new(stop.clone());
        driver.fail_on_seq = Some(3);
        let outcome = run_replay(&spec, &mut driver, &stop, LATE_THRESHOLD, |_| {});
        let f = outcome.failure.expect("应有失败");
        assert_eq!(f.round, 1);
        assert_eq!(f.seq, Some(3));
        assert_eq!(outcome.events_injected, 2);
        assert_eq!(outcome.rounds_completed, 0, "失败轮不计为完成");
    }

    #[test]
    fn severe_lateness_stops_without_backfill() {
        let events = vec![
            ev(1, 0, EventKind::MouseMove { x: 1, y: 1 }),
            ev(2, 1_000_000, EventKind::MouseMove { x: 2, y: 2 }),
            ev(3, 2_000_000, EventKind::MouseMove { x: 3, y: 3 }),
        ];
        let spec = spec_of(events, 2_000_000, 1);
        let stop = StopSignal::new();
        let mut driver = MockDriver::new(stop.clone());
        driver.lag = Duration::from_millis(1100); // 每次等待滞后 1.1s > 1000ms 阈值
        let outcome = run_replay(&spec, &mut driver, &stop, LATE_THRESHOLD, |_| {});
        let f = outcome.failure.expect("严重落后必须停止");
        assert!(f.reason.contains("落后"), "原因应说明落后: {}", f.reason);
        assert_eq!(f.seq, Some(2), "第一个超阈值事件即停止");
        assert_eq!(outcome.events_injected, 1);
        // 不补发：过期的第二个事件及其后续都未被注入。
        assert!(!outcome.timing.iter().any(|t| t.seq == 3));
    }

    #[test]
    fn stop_during_trailing_pause_marks_round_incomplete() {
        // 事件全部完成，但在结尾停顿（1000ms 处）取消。
        let events = vec![ev(1, 0, key(0x41, true)), ev(2, 100_000, key(0x41, false))];
        let spec = spec_of(events, 1_000_000, 1);
        let stop = StopSignal::new();
        let mut driver = MockDriver::new(stop.clone());
        driver.cancel_at = Some(driver.now + Duration::from_millis(500));
        let outcome = run_replay(&spec, &mut driver, &stop, LATE_THRESHOLD, |_| {});
        assert!(outcome.stopped);
        assert_eq!(outcome.events_injected, 2, "事件都已注入");
        assert_eq!(outcome.rounds_completed, 0, "结尾停顿未等完，该轮不计完成");
    }

    #[test]
    fn infinite_rounds_run_until_the_stop_signal() {
        let events = vec![ev(1, 0, key(0x41, true)), ev(2, 100_000, key(0x41, false))];
        // u32::MAX 是 UI 的无限循环哨兵；这里只验证第二轮能被停止信号截断，
        // 不会把哨兵当成非法轮数，也不会等待 40 亿轮结束。
        let spec = spec_of(events, 200_000, u32::MAX);
        let stop = StopSignal::new();
        let mut driver = MockDriver::new(stop.clone());
        driver.cancel_at = Some(driver.now + Duration::from_millis(300));
        let outcome = run_replay(&spec, &mut driver, &stop, LATE_THRESHOLD, |_| {});
        assert!(outcome.stopped);
        assert_eq!(outcome.rounds_requested, u32::MAX);
        assert_eq!(outcome.rounds_completed, 1);
    }
    #[test]
    fn validate_recording_rejects_bad_order_and_negative_time() {
        assert!(validate_recording(&[], 0).is_ok());
        let good = vec![ev(1, 0, key(0x41, true)), ev(2, 10, key(0x41, false))];
        assert!(validate_recording(&good, 100).is_ok());
        let bad_time = vec![ev(1, 50, key(0x41, true)), ev(2, 10, key(0x41, false))];
        assert!(validate_recording(&bad_time, 100).is_err());
        let bad_seq = vec![ev(2, 0, key(0x41, true)), ev(1, 10, key(0x41, false))];
        assert!(validate_recording(&bad_seq, 100).is_err());
        let negative = vec![ev(1, -5, key(0x41, true))];
        assert!(validate_recording(&negative, 100).is_err());
        let beyond = vec![ev(1, 200, key(0x41, true))];
        assert!(validate_recording(&beyond, 100).is_err());
    }

    #[test]
    fn compute_stats_quantiles_and_drift() {
        let mk = |round: u32, seq: u64, planned: i64, actual: i64| TimingRecord {
            round,
            seq,
            planned_us: planned,
            actual_us: actual,
        };
        // 100 个样本：误差 0..99us；首个 -10us、末个 +30us → 漂移 40us。
        let mut timing: Vec<TimingRecord> = (0..100)
            .map(|i| mk(1, i as u64 + 1, i * 1000, i * 1000 + i))
            .collect();
        timing[0].actual_us = timing[0].planned_us - 10;
        timing[99].actual_us = timing[99].planned_us + 30;
        let stats = compute_stats(&timing);
        assert_eq!(stats.sample_count, 100);
        // 样本 [0..98] 误差为 i；[99] 被改为 30，所以最大误差是 98。
        assert_eq!(stats.max_abs_us, 98);
        assert!(
            stats.p50_abs_us >= 45 && stats.p50_abs_us <= 55,
            "p50={}",
            stats.p50_abs_us
        );
        assert!(stats.p95_abs_us >= 90, "p95={}", stats.p95_abs_us);
        assert_eq!(stats.drift_us, 40);
        assert_eq!(stats.first_round_first_err_us, -10);
        assert_eq!(stats.first_round_last_err_us, 30);
    }
    #[test]
    fn stop_arriving_at_deadline_prevents_the_next_injection() {
        let stop = StopSignal::new();
        let mut driver = MockDriver::new(stop.clone());
        driver.stop_on_deadline = true;
        let out = run_replay(
            &spec_of(vec![ev(1, 0, key(0x41, true))], 100, 1),
            &mut driver,
            &stop,
            LATE_THRESHOLD,
            |_| {},
        );
        assert!(out.stopped);
        assert_eq!(out.events_injected, 0);
    }
    #[test]
    fn wait_and_cleanup_failures_are_reported_without_more_input() {
        let stop = StopSignal::new();
        let mut driver = MockDriver::new(stop.clone());
        driver.wait_failure = true;
        driver.cleanup_failure = true;
        let out = run_replay(
            &spec_of(vec![ev(1, 0, key(0x41, true))], 100, 1),
            &mut driver,
            &stop,
            LATE_THRESHOLD,
            |_| {},
        );
        assert_eq!(out.events_injected, 0);
        let reason = out.failure.unwrap().reason;
        assert!(reason.contains("等待失败"));
        assert!(reason.contains("释放失败"));
    }
    #[test]
    fn interrupted_session_reports_reason_and_never_resumes() {
        let stop = StopSignal::new();
        let mut driver = MockDriver::new(stop.clone());
        stop.fail("会话锁定");
        let out = run_replay(
            &spec_of(vec![ev(1, 0, key(0x41, true))], 100, 3),
            &mut driver,
            &stop,
            LATE_THRESHOLD,
            |_| {},
        );
        assert!(out.stopped);
        assert_eq!(out.events_injected, 0);
        assert_eq!(out.failure.unwrap().reason, "会话锁定");
    }
    #[test]
    fn stop_wakes_a_driver_registered_after_the_signal() {
        let stop = StopSignal::new();
        stop.signal();
        let count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let copy = count.clone();
        stop.set_waker(Arc::new(move || {
            copy.fetch_add(1, Ordering::SeqCst);
        }));
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(stop.signalled_at().is_some());
    }
}
