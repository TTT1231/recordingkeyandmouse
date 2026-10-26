//! 录制缓冲与捕获过滤（平台无关部分）。
//!
//! 钩子回调只做：复制必要字段 → 取时间戳 → `push` 入缓冲，随后立刻继续钩子链。
//! 序号在回调内按到达顺序分配，保证与实际顺序一致；同时间戳事件按序号排序。
//! 缓冲有上限（有界队列），溢出时置位标记并停止追加，由控制侧判废整次录制。

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

use crate::event::{EventKind, RecordedEvent};

/// 待处理队列上限。控制线程定期排空，整个录制没有按事件数截断；积压溢出即判废并报告。
pub const RECORD_BUFFER_CAP: usize = 2_000_000;

/// 录制控制快捷键（F8 启停录制、F9 停止执行，2026-10-26 定稿）。
pub const HOTKEY_TOGGLE_RECORD_VK: u32 = 0x77; // VK_F8
pub const HOTKEY_STOP_EXEC_VK: u32 = 0x78; // VK_F9

pub fn hotkey_vks() -> Vec<u32> {
    vec![HOTKEY_TOGGLE_RECORD_VK, HOTKEY_STOP_EXEC_VK]
}

/// 修饰键 VK 集合（含左右区分码）。用于"裸快捷键才过滤"的判断。
const MODIFIER_VKS: &[u32] = &[
    0x10, 0x11, 0x12, 0x5B, 0x5C, // Shift / Ctrl / Alt / LWin / RWin
    0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, // 左右 Shift / Ctrl / Alt
];

/// 快捷键过滤：按定稿，录制控制快捷键（裸按，无修饰键）不混入录制内容；
/// 带修饰键的同键位组合（如 Ctrl+F8）是用户真实输入，必须保留。
#[derive(Debug)]
pub struct CaptureFilter {
    hotkey_vks: Vec<u32>,
    modifiers_down: BTreeSet<u32>,
    key_decisions: BTreeMap<u32, bool>,
}

impl CaptureFilter {
    pub fn new(hotkey_vks: Vec<u32>) -> Self {
        Self {
            hotkey_vks,
            modifiers_down: BTreeSet::new(),
            key_decisions: BTreeMap::new(),
        }
    }

    fn is_modifier(vk: u32) -> bool {
        MODIFIER_VKS.contains(&vk)
    }

    /// 更新修饰键状态并判断该事件是否应记入录制。
    pub fn should_record(&mut self, kind: &EventKind) -> bool {
        if let Some(vk) = kind.vk() {
            if Self::is_modifier(vk) {
                if kind.is_key_down() {
                    self.modifiers_down.insert(vk);
                } else {
                    self.modifiers_down.remove(&vk);
                }
                return true;
            }
            if self.hotkey_vks.contains(&vk) {
                if kind.is_key_down() {
                    let keep = !self.modifiers_down.is_empty();
                    return *self.key_decisions.entry(vk).or_insert(keep);
                }
                return self.key_decisions.remove(&vk).unwrap_or(false);
            }
        }
        true
    }
}

/// 一次录制进行中的事件缓冲。由钩子线程（写）与控制侧（begin/finish）通过 Mutex 共享。
#[derive(Debug)]
pub struct RecordingBuffer {
    started: Option<Instant>,
    events: Vec<RecordedEvent>,
    next_seq: u64,
    overflow: bool,
    cap: usize,
    filter: CaptureFilter,
}

/// 停止后取出的录制结果（事件与独立时长；结尾停顿包含在 duration 内）。
#[derive(Debug)]
pub struct FinishedCapture {
    pub events: Vec<RecordedEvent>,
    pub duration_us: i64,
    pub overflow: bool,
}

impl Default for RecordingBuffer {
    fn default() -> Self {
        Self::new(hotkey_vks())
    }
}

impl RecordingBuffer {
    pub fn new(hotkey_vks: Vec<u32>) -> Self {
        Self {
            started: None,
            events: Vec::new(),
            next_seq: 1,
            overflow: false,
            cap: RECORD_BUFFER_CAP,
            filter: CaptureFilter::new(hotkey_vks),
        }
    }

    /// 开始录制：清空旧数据并记录起点。起点即"相对录制起点"的时间零点。
    pub fn begin(&mut self) {
        self.started = Some(Instant::now());
        self.events.clear();
        self.next_seq = 1;
        self.overflow = false;
        self.filter = CaptureFilter::new(self.filter.hotkey_vks.clone());
    }

    /// 钩子回调写入。未开始、被过滤、溢出后均为空操作。
    pub fn push(&mut self, at: Instant, native_time_ms: u32, kind: EventKind) {
        let Some(start) = self.started else { return };
        if self.overflow || !self.filter.should_record(&kind) {
            return;
        }
        if self.events.len() >= self.cap {
            self.overflow = true;
            return;
        }
        let time_us = (at - start).as_micros().min(i64::MAX as u128) as i64;
        self.events.push(RecordedEvent {
            seq: self.next_seq,
            time_us,
            native_time_ms,
            kind,
        });
        self.next_seq += 1;
    }

    pub fn event_count(&self) -> usize {
        self.next_seq.saturating_sub(1) as usize
    }

    pub fn drain_pending(&mut self) -> Vec<RecordedEvent> {
        std::mem::take(&mut self.events)
    }

    pub fn overflowed(&self) -> bool {
        self.overflow
    }

    /// 当前录制已进行时长（用于界面状态）。
    pub fn elapsed_us(&self) -> i64 {
        self.started
            .map(|s| {
                Instant::now()
                    .duration_since(s)
                    .as_micros()
                    .min(i64::MAX as u128) as i64
            })
            .unwrap_or(0)
    }

    /// 停止并取出结果。时长独立于最后事件时间，保留结尾停顿。
    /// 取出后 `started` 置空，迟到的回调 push 成为空操作。
    pub fn finish(&mut self) -> FinishedCapture {
        let duration_us = match self.started.take() {
            Some(start) => Instant::now()
                .duration_since(start)
                .as_micros()
                .min(i64::MAX as u128) as i64,
            None => 0,
        };
        let overflow = self.overflow;
        FinishedCapture {
            events: std::mem::take(&mut self.events),
            duration_us,
            overflow,
        }
    }

    /// 供测试注入小容量。
    #[cfg(test)]
    pub fn set_cap_for_test(&mut self, cap: usize) {
        self.cap = cap;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotkeys {
    pub record_vk: u32,
    pub stop_vk: u32,
}
impl Default for Hotkeys {
    fn default() -> Self {
        Self {
            record_vk: 0x77,
            stop_vk: 0x78,
        }
    }
}
impl Hotkeys {
    pub fn validate(&self) -> Result<(), String> {
        if !(0x70..=0x7B).contains(&self.record_vk)
            || !(0x70..=0x7B).contains(&self.stop_vk)
            || self.record_vk == self.stop_vk
        {
            Err("请选择两个不同的 F1–F12 功能键".into())
        } else {
            Ok(())
        }
    }
    pub fn vks(&self) -> Vec<u32> {
        vec![self.record_vk, self.stop_vk]
    }
}

/// 按钮触发的停止不属于录制内容。鼠标以最近的抬起配对按下，允许长按停止按钮。
pub fn trim_control_gesture(
    events: &mut Vec<RecordedEvent>,
    duration: i64,
    gesture: Option<&str>,
) -> i64 {
    let down = match gesture {
        Some("mouse") => events
            .iter()
            .rev()
            .find(|e| {
                matches!(
                    e.kind,
                    EventKind::MouseUp {
                        button: crate::event::MouseButton::Left,
                        ..
                    }
                ) && duration.saturating_sub(e.time_us) < 1_000_000
            })
            .and_then(|up| {
                events.iter().rev().find(|e| {
                    e.seq < up.seq
                        && matches!(
                            e.kind,
                            EventKind::MouseDown {
                                button: crate::event::MouseButton::Left,
                                ..
                            }
                        )
                })
            }),
        Some("enter") | Some("space") => {
            let vk = if gesture == Some("enter") { 13 } else { 32 };
            events.iter().rev().find(|e| {
                matches!(e.kind,EventKind::KeyDown{vk:actual,..} if actual==vk)
                    && duration.saturating_sub(e.time_us) < 1_000_000
            })
        }
        _ => None,
    }
    .map(|e| (e.seq, e.time_us));
    if let Some((seq, at)) = down {
        events.retain(|e| e.seq < seq);
        at
    } else {
        duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::MouseButton;

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

    #[test]
    fn bare_hotkey_keys_are_filtered() {
        let mut buf = RecordingBuffer::default();
        buf.begin();
        let t0 = Instant::now();
        buf.push(t0, 0, key(HOTKEY_TOGGLE_RECORD_VK, true));
        buf.push(t0, 0, key(HOTKEY_TOGGLE_RECORD_VK, false));
        buf.push(t0, 0, key(HOTKEY_STOP_EXEC_VK, true));
        let done = buf.finish();
        assert_eq!(done.events.len(), 0, "裸 F8/F9 不得混入录制内容");
    }

    #[test]
    fn modified_hotkey_combination_is_kept() {
        let mut buf = RecordingBuffer::default();
        buf.begin();
        let t0 = Instant::now();
        // Ctrl+F8 是真实用户输入，必须保留。
        buf.push(t0, 0, key(0xA2, true)); // 左Ctrl
        buf.push(t0, 0, key(HOTKEY_TOGGLE_RECORD_VK, true));
        buf.push(t0, 0, key(HOTKEY_TOGGLE_RECORD_VK, false));
        buf.push(t0, 0, key(0xA2, false));
        let done = buf.finish();
        assert_eq!(done.events.len(), 4);
        assert_eq!(
            done.events
                .iter()
                .filter(|e| e.kind.vk().is_some_and(|v| v == HOTKEY_TOGGLE_RECORD_VK))
                .count(),
            2
        );
    }

    #[test]
    fn seq_is_assigned_in_arrival_order_and_time_monotonic() {
        let mut buf = RecordingBuffer::default();
        buf.begin();
        let start = Instant::now();
        buf.push(start, 10, EventKind::MouseMove { x: 1, y: 2 });
        buf.push(
            start + std::time::Duration::from_micros(500),
            11,
            key(0x41, true),
        );
        // 同一时刻到达的两个事件：序号仍分先后。
        let same = start + std::time::Duration::from_micros(500);
        buf.push(
            same,
            12,
            EventKind::MouseDown {
                button: MouseButton::Left,
                x: 200,
                y: 150,
            },
        );
        buf.push(
            same,
            12,
            EventKind::MouseUp {
                button: MouseButton::Left,
                x: 200,
                y: 150,
            },
        );
        let done = buf.finish();
        let seqs: Vec<u64> = done.events.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, vec![1, 2, 3, 4]);
        let times: Vec<i64> = done.events.iter().map(|e| e.time_us).collect();
        assert!(
            times.windows(2).all(|w| w[0] <= w[1]),
            "time_us 必须非递减: {times:?}"
        );
        assert_eq!(
            done.events[2].time_us, done.events[3].time_us,
            "同时间戳事件按序号排序"
        );
    }

    #[test]
    fn push_before_begin_is_noop() {
        let mut buf = RecordingBuffer::default();
        buf.push(Instant::now(), 0, EventKind::MouseMove { x: 0, y: 0 });
        assert_eq!(buf.event_count(), 0);
    }

    #[test]
    fn overflow_marks_and_stops_appending() {
        let mut buf = RecordingBuffer::default();
        buf.set_cap_for_test(3);
        buf.begin();
        let t0 = Instant::now();
        for i in 0..5 {
            buf.push(t0, 0, EventKind::MouseMove { x: i, y: 0 });
        }
        let done = buf.finish();
        assert!(done.overflow);
        assert_eq!(done.events.len(), 3, "溢出后不再追加");
    }

    #[test]
    fn finish_keeps_trailing_pause_in_duration() {
        let mut buf = RecordingBuffer::default();
        buf.begin();
        let t0 = Instant::now();
        buf.push(t0, 0, key(0x41, true));
        // 模拟结尾停顿：60ms 后才调用 finish。
        std::thread::sleep(std::time::Duration::from_millis(60));
        let done = buf.finish();
        assert_eq!(done.events.len(), 1);
        assert!(
            done.duration_us >= 60_000,
            "duration 必须包含结尾停顿: {}us",
            done.duration_us
        );
    }
    #[test]
    fn hotkey_release_keeps_the_decision_made_on_down() {
        let mut b = RecordingBuffer::default();
        b.begin();
        let t = Instant::now();
        for event in [
            key(0xA2, true),
            key(HOTKEY_TOGGLE_RECORD_VK, true),
            key(0xA2, false),
            key(HOTKEY_TOGGLE_RECORD_VK, true),
            key(HOTKEY_TOGGLE_RECORD_VK, false),
        ] {
            b.push(t, 0, event);
        }
        assert_eq!(b.finish().events.len(), 5);
        b.begin();
        for event in [
            key(HOTKEY_TOGGLE_RECORD_VK, true),
            key(0xA2, true),
            key(HOTKEY_TOGGLE_RECORD_VK, true),
            key(HOTKEY_TOGGLE_RECORD_VK, false),
            key(0xA2, false),
        ] {
            b.push(t, 0, event);
        }
        assert_eq!(
            b.finish().events.len(),
            2,
            "裸快捷键被排除后，重复和抬起都不能变成孤立输入"
        );
    }
    #[test]
    fn draining_pending_queue_keeps_total_count_sequence_and_configuration() {
        let mut b = RecordingBuffer::new(vec![0x79, 0x7A]);
        b.set_cap_for_test(2);
        b.begin();
        let t = Instant::now();
        b.push(t, 0, key(0x79, true));
        b.push(t, 0, key(0x79, false));
        b.push(t, 0, EventKind::MouseMove { x: 1, y: 2 });
        let first = b.drain_pending();
        b.push(t, 0, EventKind::MouseMove { x: 3, y: 4 });
        b.push(t, 0, EventKind::MouseMove { x: 5, y: 6 });
        assert_eq!(b.event_count(), 3);
        let rest = b.finish();
        assert!(!rest.overflow);
        assert_eq!(first[0].seq, 1);
        assert_eq!(
            rest.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
            vec![2, 3]
        );
    }
    #[test]
    fn stop_button_excludes_the_entire_long_press_and_keeps_previous_input() {
        let mut events = vec![
            RecordedEvent {
                seq: 1,
                time_us: 0,
                native_time_ms: 0,
                kind: EventKind::MouseMove { x: 10, y: 10 },
            },
            RecordedEvent {
                seq: 2,
                time_us: 1_000_000,
                native_time_ms: 0,
                kind: EventKind::MouseDown {
                    button: MouseButton::Left,
                    x: 20,
                    y: 20,
                },
            },
            RecordedEvent {
                seq: 3,
                time_us: 4_000_000,
                native_time_ms: 0,
                kind: EventKind::MouseUp {
                    button: MouseButton::Left,
                    x: 20,
                    y: 20,
                },
            },
        ];
        let duration = trim_control_gesture(&mut events, 4_001_000, Some("mouse"));
        assert_eq!(duration, 1_000_000);
        assert_eq!(events.len(), 1);
    }
    #[test]
    fn hotkey_stop_preserves_the_tail_pause_and_unrelated_click() {
        let mut events = vec![
            RecordedEvent {
                seq: 1,
                time_us: 0,
                native_time_ms: 0,
                kind: EventKind::MouseDown {
                    button: MouseButton::Left,
                    x: 20,
                    y: 20,
                },
            },
            RecordedEvent {
                seq: 2,
                time_us: 10,
                native_time_ms: 0,
                kind: EventKind::MouseUp {
                    button: MouseButton::Left,
                    x: 20,
                    y: 20,
                },
            },
        ];
        assert_eq!(
            trim_control_gesture(&mut events, 2_000_000, None),
            2_000_000
        );
        assert_eq!(events.len(), 2);
        assert_eq!(
            trim_control_gesture(&mut events, 2_000_000, Some("mouse")),
            2_000_000
        );
        assert_eq!(events.len(), 2);
    }
}
