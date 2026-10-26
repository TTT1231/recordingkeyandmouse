//! 任务、原始记录、可编辑动作及其编译。只有这里定义编辑时间与输入状态语义。
use crate::event::{EventKind, MouseButton, RecordedEvent, WheelAxis, vk_name};
use crate::geometry::DesktopGeometry;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn wall_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}
pub fn new_id() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    format!(
        "{:x}-{:x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GestureSettings {
    pub double_click_us: i64,
    pub radius_x: i32,
    pub radius_y: i32,
}
impl Default for GestureSettings {
    fn default() -> Self {
        Self {
            double_click_us: 500_000,
            radius_x: 4,
            radius_y: 4,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawRecording {
    #[serde(default)]
    pub gestures: GestureSettings,
    pub started_at_wall_ms: i64,
    pub duration_us: i64,
    pub events: Vec<RecordedEvent>,
    pub geometry: DesktopGeometry,
    pub layout_klid: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ActionKind {
    Move {
        x: i32,
        y: i32,
    },
    Click {
        button: MouseButton,
        count: u8,
        x: i32,
        y: i32,
    },
    Drag {
        button: MouseButton,
        x: i32,
        y: i32,
    },
    Wheel {
        axis: WheelAxis,
        delta: i32,
        x: i32,
        y: i32,
    },
    Keys {
        chord: String,
    },
    Text {
        text: String,
    },
    Wait,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Action {
    pub id: String,
    pub start_us: i64,
    pub end_us: i64,
    pub kind: ActionKind,
    pub source_seqs: Vec<u64>,
    pub events: Vec<RecordedEvent>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionView {
    pub id: String,
    pub start_us: i64,
    pub end_us: i64,
    pub kind: ActionKind,
    pub title: String,
    pub detail: String,
    pub event_count: usize,
    pub editable: bool,
}
impl Action {
    pub fn view(&self) -> ActionView {
        let duration = (self.end_us - self.start_us) as f64 / 1000.0;
        let (title, detail, editable) = match &self.kind {
            ActionKind::Move { x, y } => (
                "鼠标移动".into(),
                format!("{duration:.0} 毫秒 · 移至 ({x}, {y})"),
                true,
            ),
            ActionKind::Click {
                button,
                count,
                x,
                y,
            } => (
                format!(
                    "{}（{}）",
                    if *count == 2 { "双击" } else { "单击" },
                    button.display()
                ),
                format!("({}, {})", x, y),
                !matches!(button, MouseButton::X1 | MouseButton::X2),
            ),
            ActionKind::Drag { button, x, y } => (
                format!("按住{}拖动", button.display()),
                format!("{duration:.0} 毫秒 · 至 ({x}, {y}) · 保留原始拖拽"),
                false,
            ),
            ActionKind::Wheel { axis, delta, .. } => {
                let direction = match (axis, delta >= &0) {
                    (WheelAxis::Vertical, true) => "向上",
                    (WheelAxis::Vertical, false) => "向下",
                    (WheelAxis::Horizontal, true) => "向右",
                    _ => "向左",
                };
                (
                    "鼠标滚轮".into(),
                    format!(
                        "{direction} {:.2} 格 · 1 格 = 120 原生增量",
                        delta.unsigned_abs() as f64 / 120.0
                    ),
                    *axis == WheelAxis::Vertical,
                )
            }
            ActionKind::Keys { chord } => {
                let downs: Vec<_> = self
                    .events
                    .iter()
                    .filter_map(|e| {
                        if let EventKind::KeyDown { vk, .. } = e.kind {
                            Some(vk)
                        } else {
                            None
                        }
                    })
                    .collect();
                let repeats = downs
                    .len()
                    .saturating_sub(downs.iter().collect::<BTreeSet<_>>().len());
                let detail = if repeats > 0 {
                    format!("按键 {chord} · 含 {repeats} 次重复按下")
                } else {
                    format!("按键 {chord}")
                };
                ("键盘输入".into(), detail, true)
            }
            ActionKind::Text { text } => (
                "键盘输入".into(),
                format!("输入文本 {}", text.replace('\n', "↵")),
                true,
            ),
            ActionKind::Wait => (
                "等待".into(),
                format!("{:.3} 秒 · 插入的等待", duration / 1000.0),
                true,
            ),
        };
        ActionView {
            id: self.id.clone(),
            start_us: self.start_us,
            end_us: self.end_us,
            kind: self.kind.clone(),
            title,
            detail,
            event_count: self.events.len(),
            editable,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageMapping {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceImage {
    pub name: String,
    pub data_url: String,
    pub width: u32,
    pub height: u32,
    pub mapping: ImageMapping,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDocument {
    pub id: String,
    pub name: String,
    pub revision: u64,
    pub updated_at: i64,
    pub raw: Option<RawRecording>,
    pub actions: Vec<Action>,
    pub duration_us: i64,
    pub geometry: DesktopGeometry,
    pub layout_klid: String,
    pub reference_image: Option<ReferenceImage>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub id: String,
    pub name: String,
    pub revision: u64,
    pub duration_us: i64,
    pub action_count: usize,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentView {
    pub task_id: String,
    pub name: String,
    pub revision: u64,
    pub duration_us: i64,
    pub geometry: DesktopGeometry,
    pub layout_klid: String,
    pub events: Vec<RecordedEvent>,
    pub actions: Vec<ActionView>,
    pub raw_event_count: usize,
    pub reference_image: Option<ReferenceImage>,
}
impl TaskDocument {
    pub fn new(name: String, geometry: DesktopGeometry, layout_klid: String) -> Self {
        Self {
            id: new_id(),
            name,
            revision: 1,
            updated_at: wall_ms(),
            raw: None,
            actions: Vec::new(),
            duration_us: 0,
            geometry,
            layout_klid,
            reference_image: None,
        }
    }
    pub fn replace_recording(&mut self, raw: RawRecording) {
        self.actions = group_events_with_settings(&raw.events, &raw.gestures);
        self.duration_us = raw.duration_us;
        self.geometry = raw.geometry.clone();
        self.layout_klid = raw.layout_klid.clone();
        self.raw = Some(raw);
        self.reference_image = None;
    }
    pub fn summary(&self) -> TaskSummary {
        TaskSummary {
            id: self.id.clone(),
            name: self.name.clone(),
            revision: self.revision,
            duration_us: self.duration_us,
            action_count: self.actions.len(),
        }
    }
    pub fn compile(&self) -> Vec<RecordedEvent> {
        let mut events: Vec<RecordedEvent> = self
            .actions
            .iter()
            .flat_map(|a| a.events.iter().copied())
            .collect();
        events.sort_by_key(|e| (e.time_us, e.seq));
        for (i, event) in events.iter_mut().enumerate() {
            event.seq = i as u64 + 1;
        }
        events
    }
    pub fn view(&self) -> DocumentView {
        DocumentView {
            task_id: self.id.clone(),
            name: self.name.clone(),
            revision: self.revision,
            duration_us: self.duration_us,
            geometry: self.geometry.clone(),
            layout_klid: self.layout_klid.clone(),
            events: self.compile(),
            actions: self.actions.iter().map(Action::view).collect(),
            raw_event_count: self.raw.as_ref().map(|r| r.events.len()).unwrap_or(0),
            reference_image: self.reference_image.clone(),
        }
    }
    pub fn validate_execution(&self) -> Result<(), String> {
        let events = self.compile();
        if events.is_empty() {
            return Err("任务还没有可以执行的输入".into());
        }
        crate::scheduler::validate_recording(&events, self.duration_us)?;
        validate_balanced(&events)?;
        for event in &events {
            if let Some((x, y)) = event.kind.position() {
                self.check_point(x, y)?;
            }
        }
        Ok(())
    }
    pub fn check_point(&self, x: i32, y: i32) -> Result<(), String> {
        if self.geometry.monitors.iter().any(|m| {
            i64::from(x) >= i64::from(m.x)
                && i64::from(y) >= i64::from(m.y)
                && i64::from(x) < i64::from(m.x) + i64::from(m.width)
                && i64::from(y) < i64::from(m.y) + i64::from(m.height)
        }) {
            Ok(())
        } else {
            Err("坐标不在录制时的显示器范围内".into())
        }
    }
    fn action_index(&self, id: &str) -> Result<usize, String> {
        self.actions
            .iter()
            .position(|a| a.id == id)
            .ok_or_else(|| "动作已经变化，请重新选择".into())
    }
    fn boundary(&self, after: &Option<String>) -> Result<i64, String> {
        let t = if let Some(id) = after {
            self.actions[self.action_index(id)?].end_us
        } else {
            self.duration_us
        };
        self.closed_boundary(t)?;
        Ok(t)
    }
    fn closed_boundary(&self, t: i64) -> Result<(), String> {
        self.closed_boundary_at(t, None)
    }
    fn closed_boundary_at(&self, t: i64, through_seq: Option<u64>) -> Result<(), String> {
        if self.actions.iter().any(|a| a.start_us < t && a.end_us > t) {
            return Err("此位置有跨越的移动、拖拽或按键动作，请在完整动作结束后编辑时间".into());
        }
        let events: Vec<_> = self
            .compile()
            .into_iter()
            .filter(|e| {
                e.time_us < t || (e.time_us == t && through_seq.is_none_or(|seq| e.seq <= seq))
            })
            .collect();
        if !held_state(&events, false)?.is_empty() {
            return Err("此位置仍有按住的键或鼠标，请在全部抬起后插入等待或文本".into());
        }
        Ok(())
    }
    fn shift_after(
        &mut self,
        t: i64,
        delta: i64,
        except: Option<&str>,
        through_seq: Option<u64>,
    ) -> Result<(), String> {
        for action in &mut self.actions {
            if except == Some(action.id.as_str()) {
                continue;
            }
            if action.events.is_empty() {
                if action.start_us >= t {
                    action.start_us = add_time(action.start_us, delta)?;
                    action.end_us = add_time(action.end_us, delta)?;
                }
            } else {
                let after = |event: &RecordedEvent| {
                    event.time_us > t
                        || (event.time_us == t && through_seq.is_some_and(|seq| event.seq > seq))
                };
                if action.events.iter().all(after) {
                    for event in &mut action.events {
                        event.time_us = add_time(event.time_us, delta)?;
                    }
                    action.start_us = add_time(action.start_us, delta)?;
                    action.end_us = add_time(action.end_us, delta)?;
                } else if action.events.iter().any(after) {
                    return Err("有动作跨越这条时间边界，请在完整动作结束后编辑".into());
                }
            }
        }
        self.duration_us = add_time(self.duration_us, delta)?;
        Ok(())
    }
    fn next_seq(&self) -> u64 {
        self.actions
            .iter()
            .flat_map(|a| &a.events)
            .map(|e| e.seq)
            .max()
            .unwrap_or(0)
            + 1
    }
    pub fn apply(&mut self, edit: EditRequest) -> Result<(), String> {
        let mut draft = self.clone();
        draft.apply_inner(edit)?;
        *self = draft;
        Ok(())
    }
    fn apply_inner(&mut self, edit: EditRequest) -> Result<(), String> {
        match edit {
            EditRequest::InsertWait {
                after_action,
                duration_us,
            } => {
                positive_duration(duration_us)?;
                let t = self.boundary(&after_action)?;
                self.shift_after(t, duration_us, None, None)?;
                self.actions.push(Action {
                    id: new_id(),
                    start_us: t,
                    end_us: add_time(t, duration_us)?,
                    kind: ActionKind::Wait,
                    source_seqs: vec![],
                    events: vec![],
                });
            }
            EditRequest::InsertText { after_action, text } => {
                validate_text(&text)?;
                let t = self.boundary(&after_action)?;
                let seq = self.next_seq();
                self.actions.push(Action {
                    id: new_id(),
                    start_us: t,
                    end_us: t,
                    events: text_events(&text, t, seq),
                    kind: ActionKind::Text { text },
                    source_seqs: vec![],
                });
            }
            EditRequest::UpdateWait {
                action_id,
                duration_us,
            } => {
                positive_duration(duration_us)?;
                let i = self.action_index(&action_id)?;
                if !matches!(self.actions[i].kind, ActionKind::Wait) {
                    return Err("选中的不是等待动作".into());
                }
                let old_end = self.actions[i].end_us;
                let delta = duration_us - (old_end - self.actions[i].start_us);
                self.closed_boundary_at(old_end, Some(0))?;
                self.shift_after(old_end, delta, Some(&action_id), Some(0))?;
                self.actions[i].end_us = add_time(self.actions[i].start_us, duration_us)?;
            }
            EditRequest::UpdateMove {
                action_id,
                x,
                y,
                duration_us,
                link_next_click,
            } => {
                self.check_point(x, y)?;
                if duration_us < 0 {
                    return Err("移动耗时不能小于零".into());
                }
                let i = self.action_index(&action_id)?;
                let (old_x, old_y) = match self.actions[i].kind {
                    ActionKind::Move { x, y } => (x, y),
                    _ => return Err("此动作不能作为独立移动编辑".into()),
                };
                let start = self.actions[i].start_us;
                let end = self.actions[i].end_us;
                let old_len = end - start;
                if duration_us != old_len {
                    let first_seq = self.actions[i].events.first().unwrap().seq;
                    let last_seq = self.actions[i].events.last().unwrap().seq;
                    self.closed_boundary_at(start, Some(first_seq))?;
                    self.closed_boundary_at(end, Some(last_seq))?;
                    if self.actions.iter().any(|a| {
                        a.id != action_id
                            && a.events.iter().any(|e| {
                                (e.time_us, e.seq) > (start, first_seq)
                                    && (e.time_us, e.seq) < (end, last_seq)
                            })
                    }) {
                        return Err("移动期间有其他动作，不能单独修改这段耗时".into());
                    }
                    self.shift_after(end, duration_us - old_len, Some(&action_id), Some(last_seq))?;
                }
                let n = self.actions[i].events.len();
                if n == 1 && duration_us == 0 {
                    self.actions[i].events[0].kind.set_position(x, y);
                } else if n == 1 {
                    let mut first = self.actions[i].events[0];
                    if let Some(pos) = self
                        .compile()
                        .iter()
                        .rev()
                        .find(|e| e.time_us < start && e.kind.position().is_some())
                        .and_then(|e| e.kind.position())
                    {
                        first.kind.set_position(pos.0, pos.1);
                    }
                    let mut last = first;
                    last.time_us = add_time(start, duration_us)?;
                    last.seq = first.seq;
                    last.kind.set_position(x, y);
                    self.actions[i].events = vec![first, last];
                } else {
                    for (j, event) in self.actions[i].events.iter_mut().enumerate() {
                        let part = if old_len > 0 {
                            (event.time_us - start) as f64 / old_len as f64
                        } else {
                            j as f64 / (n - 1) as f64
                        };
                        if let Some((px, py)) = event.kind.position() {
                            event.kind.set_position(
                                (f64::from(px) + (f64::from(x) - f64::from(old_x)) * part).round()
                                    as i32,
                                (f64::from(py) + (f64::from(y) - f64::from(old_y)) * part).round()
                                    as i32,
                            );
                        }
                        event.time_us = start + (duration_us as f64 * part).round() as i64;
                    }
                }
                self.actions[i].kind = ActionKind::Move { x, y };
                self.actions[i].end_us = add_time(start, duration_us)?;
                if link_next_click {
                    let next = self
                        .actions
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| a.start_us >= self.actions[i].end_us && a.id != action_id)
                        .min_by_key(|(_, a)| a.start_us)
                        .map(|(j, _)| j);
                    let j = next.ok_or("没有可以联动的后续点击")?;
                    match &mut self.actions[j].kind {
                        ActionKind::Click {
                            x: cx,
                            y: cy,
                            button: MouseButton::Left | MouseButton::Right | MouseButton::Middle,
                            ..
                        } => {
                            *cx = x;
                            *cy = y;
                            for event in &mut self.actions[j].events {
                                event.kind.set_position(x, y);
                            }
                        }
                        _ => return Err("紧接着的动作不是可编辑点击，请取消联动".into()),
                    }
                }
            }
            EditRequest::UpdateClick {
                action_id,
                button,
                count,
                x,
                y,
            } => {
                self.check_point(x, y)?;
                if matches!(button, MouseButton::X1 | MouseButton::X2) || !(1..=2).contains(&count)
                {
                    return Err("点击编辑只支持左、右、中键及单击或双击".into());
                }
                let i = self.action_index(&action_id)?;
                if !matches!(
                    self.actions[i].kind,
                    ActionKind::Click {
                        button: MouseButton::Left | MouseButton::Right | MouseButton::Middle,
                        ..
                    }
                ) {
                    return Err("此动作不能编辑为点击".into());
                }
                let a = &mut self.actions[i];
                let first = a.events.first().map(|e| e.seq).unwrap_or(1);
                let last = a.events.last().map(|e| e.seq).unwrap_or(first);
                let n = usize::from(count) * 2;
                a.events = (0..n)
                    .map(|j| RecordedEvent {
                        seq: if j + 1 == n { last } else { first },
                        time_us: a.start_us + (a.end_us - a.start_us) * j as i64 / (n - 1) as i64,
                        native_time_ms: 0,
                        kind: if j % 2 == 0 {
                            EventKind::MouseDown { button, x, y }
                        } else {
                            EventKind::MouseUp { button, x, y }
                        },
                    })
                    .collect();
                a.kind = ActionKind::Click {
                    button,
                    count,
                    x,
                    y,
                };
            }
            EditRequest::UpdateWheel { action_id, delta } => {
                if delta == 0 || delta == i32::MIN {
                    return Err("滚动量必须为非零原生增量".into());
                }
                let i = self.action_index(&action_id)?;
                let (x, y) = match self.actions[i].kind {
                    ActionKind::Wheel {
                        axis: WheelAxis::Vertical,
                        x,
                        y,
                        ..
                    } => (x, y),
                    _ => return Err("此动作不支持滚轮参数编辑".into()),
                };
                let len = self.actions[i].events.len();
                let mut emitted = 0;
                for (j, e) in self.actions[i].events.iter_mut().enumerate() {
                    let to = (i64::from(delta) * (j as i64 + 1) / len as i64) as i32;
                    if let EventKind::Wheel { delta: amount, .. } = &mut e.kind {
                        *amount = to - emitted;
                    }
                    emitted = to;
                }
                self.actions[i].kind = ActionKind::Wheel {
                    axis: WheelAxis::Vertical,
                    delta,
                    x,
                    y,
                };
            }
            EditRequest::UpdateKeys { action_id, chord } => {
                let vks = parse_chord(&chord)?;
                let i = self.action_index(&action_id)?;
                if !matches!(
                    self.actions[i].kind,
                    ActionKind::Keys { .. } | ActionKind::Text { .. }
                ) {
                    return Err("选中的不是按键动作".into());
                }
                let a = &mut self.actions[i];
                let seq = a.events.first().map(|e| e.seq).unwrap_or(1);
                let last = a.events.last().map(|e| e.seq).unwrap_or(seq);
                a.events = vks
                    .iter()
                    .map(|vk| RecordedEvent {
                        seq,
                        time_us: a.start_us,
                        native_time_ms: 0,
                        kind: EventKind::KeyDown {
                            vk: *vk,
                            scan: 0,
                            extended: is_extended_vk(*vk),
                        },
                    })
                    .chain(vks.iter().rev().map(|vk| RecordedEvent {
                        seq: last,
                        time_us: a.end_us,
                        native_time_ms: 0,
                        kind: EventKind::KeyUp {
                            vk: *vk,
                            scan: 0,
                            extended: is_extended_vk(*vk),
                        },
                    }))
                    .collect();
                a.kind = ActionKind::Keys {
                    chord: vks
                        .iter()
                        .map(|v| vk_name(*v))
                        .collect::<Vec<_>>()
                        .join(" + "),
                };
            }
            EditRequest::UpdateText { action_id, text } => {
                validate_text(&text)?;
                let i = self.action_index(&action_id)?;
                if !matches!(
                    self.actions[i].kind,
                    ActionKind::Text { .. } | ActionKind::Keys { .. }
                ) {
                    return Err("此动作不能编辑为文本".into());
                }
                let seq = self.actions[i]
                    .events
                    .first()
                    .map(|e| e.seq)
                    .unwrap_or(self.next_seq());
                self.actions[i].events = text_events(&text, self.actions[i].start_us, seq);
                for event in &mut self.actions[i].events {
                    event.seq = seq;
                }
                self.actions[i].kind = ActionKind::Text { text };
            }
            EditRequest::SetReference { reference } => {
                if let Some(image) = &reference {
                    use base64::Engine;
                    if image.width == 0
                        || image.height == 0
                        || image.mapping.width <= 0
                        || image.mapping.height <= 0
                    {
                        return Err("参考图尺寸无效".into());
                    }
                    let (header, data) = image.data_url.split_once(',').ok_or("参考图格式无效")?;
                    if ![
                        "data:image/png;base64",
                        "data:image/jpeg;base64",
                        "data:image/webp;base64",
                    ]
                    .contains(&header)
                    {
                        return Err("请选择 PNG、JPEG 或 WebP 图片".into());
                    }
                    if data.len() > 28_000_000 {
                        return Err("参考图不能超过 20 MB".into());
                    }
                    base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|_| "参考图数据损坏")?;
                }
                self.reference_image = reference;
            }
        }
        // 当前事件序号是排序标尺；原始来源序号保存在 source_seqs 中，永不重写。
        let mut order: Vec<_> = self
            .actions
            .iter()
            .enumerate()
            .flat_map(|(a, action)| {
                action
                    .events
                    .iter()
                    .enumerate()
                    .map(move |(e, event)| (event.time_us, event.seq, a, e))
            })
            .collect();
        order.sort_by_key(|&(t, seq, _, _)| (t, seq));
        for (rank, &(_, _, a, e)) in order.iter().enumerate() {
            self.actions[a].events[e].seq = rank as u64 + 1;
        }
        self.actions.sort_by_key(|a| {
            (
                a.start_us,
                a.events.first().map(|e| e.seq).unwrap_or(u64::MAX),
            )
        });
        for event in self.compile() {
            if event.time_us < 0 || event.time_us > self.duration_us {
                return Err("编辑产生了越界时间，已取消修改".into());
            }
            if let Some((x, y)) = event.kind.position() {
                self.check_point(x, y)?;
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum EditRequest {
    InsertWait {
        after_action: Option<String>,
        duration_us: i64,
    },
    InsertText {
        after_action: Option<String>,
        text: String,
    },
    UpdateWait {
        action_id: String,
        duration_us: i64,
    },
    UpdateMove {
        action_id: String,
        x: i32,
        y: i32,
        duration_us: i64,
        link_next_click: bool,
    },
    UpdateClick {
        action_id: String,
        button: MouseButton,
        count: u8,
        x: i32,
        y: i32,
    },
    UpdateWheel {
        action_id: String,
        delta: i32,
    },
    UpdateKeys {
        action_id: String,
        chord: String,
    },
    UpdateText {
        action_id: String,
        text: String,
    },
    SetReference {
        reference: Option<ReferenceImage>,
    },
}
fn add_time(t: i64, delta: i64) -> Result<i64, String> {
    t.checked_add(delta)
        .filter(|v| *v >= 0)
        .ok_or_else(|| "时间超出有效范围".into())
}
fn is_extended_vk(vk: u32) -> bool {
    matches!(vk, 0x21..=0x28 | 0x2D..=0x2E | 0x5B..=0x5C | 0xA3 | 0xA5)
}
fn positive_duration(duration: i64) -> Result<(), String> {
    if duration > 0 {
        Ok(())
    } else {
        Err("耗时必须大于零".into())
    }
}
fn validate_text(text: &str) -> Result<(), String> {
    if text.is_empty() || text.len() > 1_000_000 {
        Err("请输入文本，单个文本动作不能超过 1 MB".into())
    } else {
        Ok(())
    }
}
fn text_events(text: &str, t: i64, seq: u64) -> Vec<RecordedEvent> {
    text.encode_utf16()
        .enumerate()
        .flat_map(|(i, unit)| {
            [false, true]
                .into_iter()
                .enumerate()
                .map(move |(j, up)| RecordedEvent {
                    seq: seq + (i * 2 + j) as u64,
                    time_us: t,
                    native_time_ms: 0,
                    kind: EventKind::Unicode { unit, up },
                })
        })
        .collect()
}
fn held_state(events: &[RecordedEvent], strict: bool) -> Result<BTreeSet<String>, String> {
    let mut held = BTreeSet::new();
    for e in events {
        let state = match e.kind {
            EventKind::KeyDown { vk, .. } => Some((format!("k{vk}"), false)),
            EventKind::KeyUp { vk, .. } => Some((format!("k{vk}"), true)),
            EventKind::MouseDown { button, .. } => Some((format!("m{button:?}"), false)),
            EventKind::MouseUp { button, .. } => Some((format!("m{button:?}"), true)),
            EventKind::Unicode { unit, up } => Some((format!("u{unit}"), up)),
            _ => None,
        };
        if let Some((key, up)) = state {
            if up {
                if !held.remove(&key) && strict {
                    return Err("录制包含没有对应按下的抬起事件，请松开所有键后重新录制".into());
                }
            } else if !held.insert(key) && strict && !matches!(e.kind, EventKind::KeyDown { .. }) {
                return Err("录制包含重复按下的鼠标或字符事件，无法组成完整输入".into());
            }
        }
    }
    Ok(held)
}
pub fn validate_balanced(events: &[RecordedEvent]) -> Result<(), String> {
    if held_state(events, true)?.is_empty() {
        Ok(())
    } else {
        Err("录制结束时仍有按住的键或鼠标，不能直接循环执行；请全部抬起后重新录制".into())
    }
}
pub fn parse_chord(chord: &str) -> Result<Vec<u32>, String> {
    let mut result = Vec::new();
    for part in chord.split('+') {
        let key = part.trim().to_uppercase();
        let vk = match key.as_str() {
            "CTRL" | "左CTRL" => 0xA2,
            "右CTRL" => 0xA3,
            "SHIFT" | "左SHIFT" => 0xA0,
            "右SHIFT" => 0xA1,
            "ALT" | "左ALT" => 0xA4,
            "右ALT" => 0xA5,
            "WIN" | "左WIN" => 0x5B,
            "右WIN" => 0x5C,
            "ENTER" | "回车" => 0x0D,
            "TAB" => 0x09,
            "SPACE" | "空格" => 0x20,
            "ESC" | "ESCAPE" => 0x1B,
            "BACKSPACE" => 0x08,
            "DELETE" => 0x2E,
            "INSERT" => 0x2D,
            "HOME" => 0x24,
            "END" => 0x23,
            "PAGEUP" => 0x21,
            "PAGEDOWN" => 0x22,
            "LEFT" | "←" => 0x25,
            "UP" | "↑" => 0x26,
            "RIGHT" | "→" => 0x27,
            "DOWN" | "↓" => 0x28,
            _ if key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric() => {
                u32::from(key.as_bytes()[0])
            }
            _ if key.starts_with('F') => key[1..]
                .parse::<u32>()
                .ok()
                .filter(|n| (1..=24).contains(n))
                .map(|n| 0x6F + n)
                .ok_or("不认识这个功能键")?,
            _ => return Err("请输入按键名称，例如 A、Tab、Ctrl + S；输入文字请切换到文本".into()),
        };
        if result.contains(&vk) {
            return Err("组合键不能重复同一个键".into());
        }
        result.push(vk);
    }
    if result.is_empty() {
        Err("请输入按键".into())
    } else {
        Ok(result)
    }
}

/// 每条原始事件只归属一个动作；鼠标按住时的键盘事件仍独立处理。
pub fn group_events(events: &[RecordedEvent]) -> Vec<Action> {
    group_events_with_settings(events, &GestureSettings::default())
}
pub fn group_events_with_settings(
    events: &[RecordedEvent],
    gestures: &GestureSettings,
) -> Vec<Action> {
    let mut rows: Vec<Action> = Vec::new();
    let mut buttons: BTreeMap<MouseButton, usize> = BTreeMap::new();
    let mut keys = BTreeSet::new();
    let mut key_row = None;
    let mut last_move: Option<usize> = None;
    let mut last_wheel: Option<usize> = None;
    for event in events {
        let mut target = None;
        match event.kind {
            EventKind::MouseMove { x, y } => {
                if let Some((_, index)) = buttons.iter().next() {
                    target = Some(*index);
                    for (&button, &index) in &buttons {
                        mark_drag(&mut rows[index], button, x, y);
                    }
                } else if let Some(index) = last_move
                    && event.time_us - rows[index].end_us <= 120_000
                {
                    target = Some(index);
                    rows[index].kind = ActionKind::Move { x, y };
                }
                if target.is_none() {
                    rows.push(new_action(event, ActionKind::Move { x, y }));
                    last_move = Some(rows.len() - 1);
                }
                last_wheel = None;
            }
            EventKind::MouseDown { button, x, y } => {
                rows.push(new_action(
                    event,
                    ActionKind::Click {
                        button,
                        count: 1,
                        x,
                        y,
                    },
                ));
                buttons.insert(button, rows.len() - 1);
                last_move = None;
                last_wheel = None;
            }
            EventKind::MouseUp { button, x, y } => {
                if let Some(index) = buttons.remove(&button) {
                    mark_drag(&mut rows[index], button, x, y);
                    target = Some(index);
                } else {
                    rows.push(new_action(
                        event,
                        ActionKind::Click {
                            button,
                            count: 1,
                            x,
                            y,
                        },
                    ));
                }
                last_move = None;
                last_wheel = None;
            }
            EventKind::Wheel { axis, delta, x, y } => {
                if let Some(index) = last_wheel
                    && event.time_us - rows[index].end_us <= 150_000
                    && let ActionKind::Wheel {
                        axis: old_axis,
                        delta: total,
                        ..
                    } = &mut rows[index].kind
                    && *old_axis == axis
                    && total.signum() == delta.signum()
                    && let Some(sum) = total.checked_add(delta)
                {
                    *total = sum;
                    if let ActionKind::Wheel { x: px, y: py, .. } = &mut rows[index].kind {
                        *px = x;
                        *py = y;
                    }
                    target = Some(index);
                }
                if target.is_none() {
                    rows.push(new_action(event, ActionKind::Wheel { axis, delta, x, y }));
                    last_wheel = Some(rows.len() - 1);
                }
                last_move = None;
            }
            EventKind::KeyDown { vk, .. } => {
                if let Some(index) = key_row {
                    target = Some(index);
                    if !keys.contains(&vk)
                        && let ActionKind::Keys { chord } = &mut rows[index].kind
                    {
                        chord.push_str(&format!(" + {}", vk_name(vk)));
                    }
                } else {
                    rows.push(new_action(event, ActionKind::Keys { chord: vk_name(vk) }));
                    key_row = Some(rows.len() - 1);
                }
                keys.insert(vk);
                last_move = None;
                last_wheel = None;
            }
            EventKind::KeyUp { vk, .. } => {
                if let Some(index) = key_row {
                    target = Some(index);
                } else {
                    rows.push(new_action(
                        event,
                        ActionKind::Keys {
                            chord: format!("{} 抬起（录制前已按住）", vk_name(vk)),
                        },
                    ));
                }
                keys.remove(&vk);
                if keys.is_empty() {
                    key_row = None;
                }
                last_move = None;
                last_wheel = None;
            }
            EventKind::Unicode { .. } => {
                rows.push(new_action(
                    event,
                    ActionKind::Keys {
                        chord: "字符输入".into(),
                    },
                ));
                last_move = None;
                last_wheel = None;
            }
        }
        if let Some(index) = target {
            rows[index].events.push(*event);
            rows[index].source_seqs.push(event.seq);
            rows[index].end_us = event.time_us;
        }
    }
    let mut i = 0;
    while i < rows.len() {
        if let ActionKind::Click {
            button,
            count: 1,
            x,
            y,
        } = rows[i].kind
            && is_complete_click(&rows[i])
        {
            let next = ((i + 1)..rows.len()).find(|j| {
                matches!(
                    rows[*j].kind,
                    ActionKind::Click { .. }
                        | ActionKind::Drag { .. }
                        | ActionKind::Keys { .. }
                        | ActionKind::Wheel { .. }
                )
            });
            if let Some(j) = next
                && let ActionKind::Click {
                    button: other,
                    count: 1,
                    x: ox,
                    y: oy,
                } = rows[j].kind
                && other == button
                && is_complete_click(&rows[j])
                && rows[j].start_us - rows[i].start_us <= gestures.double_click_us
                && (i64::from(x) - i64::from(ox)).abs() <= i64::from(gestures.radius_x)
                && (i64::from(y) - i64::from(oy)).abs() <= i64::from(gestures.radius_y)
            {
                let second = rows.remove(j);
                rows[i].kind = ActionKind::Click {
                    button,
                    count: 2,
                    x,
                    y,
                };
                rows[i].end_us = second.end_us;
                rows[i].events.extend(second.events);
                rows[i].source_seqs.extend(second.source_seqs);
            }
        }
        i += 1;
    }
    absorb_coincident_moves(&mut rows);
    rows
}
fn new_action(event: &RecordedEvent, kind: ActionKind) -> Action {
    Action {
        id: new_id(),
        start_us: event.time_us,
        end_us: event.time_us,
        kind,
        source_seqs: vec![event.seq],
        events: vec![*event],
    }
}

fn mark_drag(action: &mut Action, button: MouseButton, x: i32, y: i32) {
    let moved = action
        .events
        .first()
        .and_then(|e| e.kind.position())
        .is_some_and(|(sx, sy)| {
            (i64::from(x) - i64::from(sx)).abs() > 4 || (i64::from(y) - i64::from(sy)).abs() > 4
        });
    if moved || matches!(action.kind, ActionKind::Drag { .. }) {
        action.kind = ActionKind::Drag { button, x, y };
    }
}
fn is_complete_click(action: &Action) -> bool {
    action
        .events
        .iter()
        .filter(|e| {
            matches!(
                e.kind,
                EventKind::MouseDown { .. } | EventKind::MouseUp { .. }
            )
        })
        .count()
        == 2
        && matches!(
            action.events.first().map(|e| e.kind),
            Some(EventKind::MouseDown { .. })
        )
        && matches!(
            action.events.last().map(|e| e.kind),
            Some(EventKind::MouseUp { .. })
        )
}

/// 点击/滚轮可能伴随同位置的移动通知；一条无耗时通知归入对应动作，保留原始序列。
fn absorb_coincident_moves(rows: &mut Vec<Action>) {
    let mut i = 0;
    while i < rows.len() {
        let candidate =
            matches!(rows[i].kind, ActionKind::Move { .. }) && rows[i].events.len() == 1;
        if !candidate {
            i += 1;
            continue;
        }
        let point = rows[i].events[0].kind.position();
        let at = rows[i].start_us;
        let matches = |a: &Action| {
            matches!(
                a.kind,
                ActionKind::Click { .. } | ActionKind::Wheel { .. } | ActionKind::Drag { .. }
            ) && a.events.iter().any(|e| e.kind.position() == point)
        };
        let target = if i > 0
            && matches(&rows[i - 1])
            && at >= rows[i - 1].start_us
            && at <= rows[i - 1].end_us
        {
            Some(i - 1)
        } else if i + 1 < rows.len()
            && matches(&rows[i + 1])
            && rows[i + 1].start_us >= at
            && rows[i + 1].start_us - at <= 8_000
        {
            Some(i + 1)
        } else {
            None
        };
        if let Some(target) = target {
            let row = rows.remove(i);
            let target = if target > i { target - 1 } else { target };
            let action = &mut rows[target];
            action.start_us = action.start_us.min(row.start_us);
            action.end_us = action.end_us.max(row.end_us);
            action.events.extend(row.events);
            action.events.sort_by_key(|e| (e.time_us, e.seq));
            action.source_seqs.extend(row.source_seqs);
            action.source_seqs.sort();
        } else {
            i += 1;
        }
    }
}

#[cfg(test)]
#[path = "document_tests.rs"]
mod tests;
