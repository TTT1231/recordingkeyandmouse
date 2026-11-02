//! 事件模型：原始输入事件与其承载的数据。
//!
//! 约束（技术架构决策第 5 节）：时间使用相对录制起点的整数微秒，配合事件序号共同确定顺序；
//! 按下/抬起、坐标、滚轮原生增量必须完整保留；面向用户的动作归并由 document 模块负责。

use serde::{Deserialize, Serialize};

/// 鼠标按键。侧键（X1/X2）如实捕获与回放，当前不提供参数编辑。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    X1,
    X2,
}

impl MouseButton {
    pub fn display(self) -> &'static str {
        match self {
            MouseButton::Left => "左键",
            MouseButton::Right => "右键",
            MouseButton::Middle => "中键",
            MouseButton::X1 => "侧键1",
            MouseButton::X2 => "侧键2",
        }
    }
}

/// 滚轮轴。delta 保存平台原生增量（Windows 通常每格为 WHEEL_DELTA=120），不做单位换算。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WheelAxis {
    Vertical,
    Horizontal,
}

impl WheelAxis {
    /// 收尾报告/描述用（当前由测试与报告路径使用）。
    #[allow(dead_code)]
    pub fn display(self) -> &'static str {
        match self {
            WheelAxis::Vertical => "垂直",
            WheelAxis::Horizontal => "水平",
        }
    }
}

/// 单条原始输入事件。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EventKind {
    KeyDown {
        vk: u32,
        scan: u32,
        extended: bool,
    },
    KeyUp {
        vk: u32,
        scan: u32,
        extended: bool,
    },
    MouseMove {
        x: i32,
        y: i32,
    },
    MouseDown {
        button: MouseButton,
        x: i32,
        y: i32,
    },
    MouseUp {
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
    /// 用户明确新增的文本，编译为 UTF-16 按下/抬起；不是录制所得的文本。
    Unicode {
        unit: u16,
        up: bool,
    },
}

impl EventKind {
    pub fn position(&self) -> Option<(i32, i32)> {
        match self {
            Self::MouseMove { x, y }
            | Self::MouseDown { x, y, .. }
            | Self::MouseUp { x, y, .. }
            | Self::Wheel { x, y, .. } => Some((*x, *y)),
            _ => None,
        }
    }

    pub fn set_position(&mut self, px: i32, py: i32) {
        match self {
            Self::MouseMove { x, y }
            | Self::MouseDown { x, y, .. }
            | Self::MouseUp { x, y, .. }
            | Self::Wheel { x, y, .. } => {
                *x = px;
                *y = py;
            }
            _ => {}
        }
    }
    pub fn vk(&self) -> Option<u32> {
        match self {
            EventKind::KeyDown { vk, .. } | EventKind::KeyUp { vk, .. } => Some(*vk),
            _ => None,
        }
    }

    pub fn is_key_down(&self) -> bool {
        matches!(self, EventKind::KeyDown { .. })
    }

    /// 调度测试与后续报告路径使用。
    #[allow(dead_code)]
    pub fn is_key_up(&self) -> bool {
        matches!(self, EventKind::KeyUp { .. })
    }

    /// 用于执行收尾报告的简短描述；只描述事件本身，不推测目标。
    #[allow(dead_code)]
    pub fn describe(&self) -> String {
        match self {
            EventKind::KeyDown { vk, .. } => format!("按下 {}", vk_name(*vk)),
            EventKind::KeyUp { vk, .. } => format!("抬起 {}", vk_name(*vk)),
            EventKind::MouseMove { x, y } => format!("移动到 ({x}, {y})"),
            EventKind::MouseDown { button, .. } => format!("按下鼠标{}", button.display()),
            EventKind::MouseUp { button, .. } => format!("抬起鼠标{}", button.display()),
            EventKind::Unicode { .. } => "明确文本输入".into(),
            EventKind::Wheel { axis, delta, .. } => {
                format!(
                    "{}滚轮 {}",
                    axis.display(),
                    if *delta >= 0 { "前/右" } else { "后/左" }
                )
            }
        }
    }
}

/// 从 Windows MSLLHOOKSTRUCT.mouseData 提取滚轮增量（高 16 位有符号数）。
pub fn wheel_delta_from_mouse_data(mouse_data: u32) -> i32 {
    ((mouse_data >> 16) & 0xFFFF) as u16 as i16 as i32
}

/// 常见虚拟键码到可读名称。仅用于展示与收尾报告；识别不了时显示 `#码`，不猜业务含义。
pub fn vk_name(vk: u32) -> String {
    let name = match vk {
        0x08 => "Backspace",
        0x09 => "Tab",
        0x0D => "Enter",
        0x10 => "Shift",
        0x11 => "Ctrl",
        0x12 => "Alt",
        0x13 => "Pause",
        0x14 => "CapsLock",
        0x1B => "Esc",
        0x20 => "空格",
        0x21 => "PageUp",
        0x22 => "PageDown",
        0x23 => "End",
        0x24 => "Home",
        0x25 => "←",
        0x26 => "↑",
        0x27 => "→",
        0x28 => "↓",
        0x2C => "PrtSc",
        0x2D => "Insert",
        0x2E => "Delete",
        0x30..=0x39 => return char::from(b'0' + (vk as u8 - b'0')).to_string(),
        0x41..=0x5A => return char::from(b'A' + (vk as u8 - b'A')).to_string(),
        0x5B => "左Win",
        0x5C => "右Win",
        0x5D => "菜单",
        0x60..=0x69 => return format!("小键盘{}", vk - 0x60),
        0x6A => "小键盘*",
        0x6B => "小键盘+",
        0x6D => "小键盘-",
        0x6E => "小键盘.",
        0x6F => "小键盘/",
        0x70..=0x87 => return format!("F{}", vk - 0x6F),
        0xA0 => "左Shift",
        0xA1 => "右Shift",
        0xA2 => "左Ctrl",
        0xA3 => "右Ctrl",
        0xA4 => "左Alt",
        0xA5 => "右Alt",
        0xBA => ";",
        0xBB => "=",
        0xBC => ",",
        0xBD => "-",
        0xBE => ".",
        0xBF => "/",
        0xC0 => "`",
        0xDB => "[",
        0xDC => "\\",
        0xDD => "]",
        0xDE => "'",
        _ => return format!("#0x{vk:02X}"),
    };
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_delta_extract_keeps_sign_and_magnitude() {
        assert_eq!(wheel_delta_from_mouse_data(120u32 << 16), 120);
        assert_eq!(
            wheel_delta_from_mouse_data(((-120i16) as u16 as u32) << 16),
            -120
        );
        assert_eq!(wheel_delta_from_mouse_data(240u32 << 16), 240);
        assert_eq!(wheel_delta_from_mouse_data(0), 0);
        assert_eq!(wheel_delta_from_mouse_data(0x8000_0000), -32768);
        assert_eq!(wheel_delta_from_mouse_data(0xFFFF_0000), -1);
    }

    #[test]
    fn wheel_delta_high_word_only() {
        // 低 16 位（如 X 按钮、杂项标志）不得污染滚轮增量。
        let raw = ((-120i16) as u16 as u32) << 16 | 0x1234;
        assert_eq!(wheel_delta_from_mouse_data(raw), -120);
    }

    #[test]
    fn serde_event_kind_roundtrip() {
        let ev = EventKind::KeyDown {
            vk: 0x41,
            scan: 0x1E,
            extended: false,
        };
        let s = serde_json::to_string(&ev).unwrap();
        assert!(s.contains(r#""kind":"keyDown""#), "tag 应为 camelCase: {s}");
        let back: EventKind = serde_json::from_str(&s).unwrap();
        assert_eq!(ev, back);
    }

    #[test]
    fn serde_recorded_event_camel_case_fields() {
        let rec = crate::event::RecordedEvent {
            seq: 7,
            time_us: 1_234_567,
            native_time_ms: 89,
            kind: EventKind::Wheel {
                axis: WheelAxis::Vertical,
                delta: -240,
                x: 200,
                y: 150,
            },
        };
        let s = serde_json::to_string(&rec).unwrap();
        assert!(s.contains(r#""timeUs":1234567"#), "字段应为 camelCase: {s}");
        assert!(s.contains(r#""nativeTimeMs":89"#));
        let back: RecordedEvent = serde_json::from_str(&s).unwrap();
        assert_eq!(rec, back);
    }

    #[test]
    fn vk_name_covers_common_keys() {
        assert_eq!(vk_name(0x41), "A");
        assert_eq!(vk_name(0x77), "F8");
        assert_eq!(vk_name(0x09), "Tab");
        assert_eq!(vk_name(0x1234), "#0x1234");
    }
}

/// 录制完成的一条事件：序号 + 相对微秒 + 原生时间（诊断用）+ 内容。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedEvent {
    pub seq: u64,
    pub time_us: i64,
    pub native_time_ms: u32,
    pub kind: EventKind,
}
