//! 桌面几何：显示器布局快照与注入坐标换算。
//!
//! 约束（ADR-0001 第 5.5 节）：坐标记录附带桌面原点与显示器布局；Windows 统一使用
//! 物理像素（进程声明 Per-Monitor V2 感知），支持虚拟桌面负坐标；前端 CSS 坐标不得
//! 直接送入注入，必须经由这里的虚拟桌面归一化换算。

use serde::{Deserialize, Serialize};

/// 单台显示器（物理像素，坐标为该显示器左上角在虚拟桌面中的位置）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub dpi: u32,
}

/// 一次录制/执行时的显示器布局快照。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DesktopGeometry {
    pub monitors: Vec<MonitorInfo>,
}

/// 虚拟桌面矩形 (x, y, width, height)。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VirtualScreen {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl DesktopGeometry {
    pub fn virtual_screen(&self) -> Option<VirtualScreen> {
        let first = self.monitors.first()?;
        let mut x = first.x;
        let mut y = first.y;
        let mut right = first.x + first.width;
        let mut bottom = first.y + first.height;
        for m in &self.monitors[1..] {
            x = x.min(m.x);
            y = y.min(m.y);
            right = right.max(m.x + m.width);
            bottom = bottom.max(m.y + m.height);
        }
        Some(VirtualScreen {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }

    /// 执行前检查：布局与录制时不一致则阻止执行（2026-10-26 定稿：不默默按比例猜测新目标）。
    /// 只比较物理矩形与名称；DPI 变化不影响物理坐标，单独提示。
    pub fn same_layout(&self, other: &DesktopGeometry) -> bool {
        let strip = |g: &DesktopGeometry| -> Vec<(String, i32, i32, i32, i32)> {
            g.monitors
                .iter()
                .map(|m| (m.name.clone(), m.x, m.y, m.width, m.height))
                .collect()
        };
        // 排序后比较，避免枚举顺序差异造成误报。
        let mut a = strip(self);
        let mut b = strip(other);
        a.sort();
        b.sort();
        a == b
    }
}

/// SendInput `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK` 的归一化换算：
/// 将虚拟桌面物理坐标映射到 0..65535。这是纯函数，平台层注入前调用。
pub fn to_normalized_absolute(x: i32, y: i32, virt: &VirtualScreen) -> (i32, i32) {
    let nx = normalize_axis(x, virt.x, virt.width);
    let ny = normalize_axis(y, virt.y, virt.height);
    (nx, ny)
}

fn normalize_axis(v: i32, origin: i32, extent: i32) -> i32 {
    if extent <= 1 {
        return 0;
    }
    let scaled = ((v as i64 - origin as i64) * 65535) / (extent as i64 - 1);
    i32::try_from(scaled).unwrap_or(0).clamp(0, 65535)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single_screen() -> DesktopGeometry {
        DesktopGeometry {
            monitors: vec![MonitorInfo {
                name: "\\\\.\\DISPLAY1".into(),
                x: 0,
                y: 0,
                width: 1920,
                height: 1080,
                dpi: 96,
            }],
        }
    }

    #[test]
    fn single_screen_endpoints_map_to_full_range() {
        let g = single_screen();
        let v = g.virtual_screen().unwrap();
        assert_eq!(to_normalized_absolute(0, 0, &v), (0, 0));
        assert_eq!(to_normalized_absolute(1919, 1079, &v), (65535, 65535));
        // 中点附近合理（整数截断：960*65535/1919 = 32784）。
        let (cx, _) = to_normalized_absolute(960, 540, &v);
        assert!((32783..=32785).contains(&cx), "cx={cx}");
    }

    #[test]
    fn negative_origin_virtual_screen_maps_to_full_range() {
        // 主屏在右、副屏在左（负坐标）的布局。
        let g = DesktopGeometry {
            monitors: vec![
                MonitorInfo {
                    name: "\\\\.\\DISPLAY1".into(),
                    x: 0,
                    y: 0,
                    width: 1920,
                    height: 1080,
                    dpi: 96,
                },
                MonitorInfo {
                    name: "\\\\.\\DISPLAY2".into(),
                    x: -1920,
                    y: 0,
                    width: 1920,
                    height: 1080,
                    dpi: 96,
                },
            ],
        };
        let v = g.virtual_screen().unwrap();
        assert_eq!((v.x, v.width), (-1920, 3840));
        assert_eq!(to_normalized_absolute(-1920, 0, &v).0, 0);
        assert_eq!(to_normalized_absolute(1919, 0, &v).0, 65535);
        // 主屏左上角应落在虚拟桌面中点（1920*65535/3839 = 32776）。
        let (mid, _) = to_normalized_absolute(0, 0, &v);
        assert!((32775..=32777).contains(&mid), "mid={mid}");
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let g = single_screen();
        let v = g.virtual_screen().unwrap();
        assert_eq!(to_normalized_absolute(-500, -500, &v), (0, 0));
        assert_eq!(to_normalized_absolute(3000, 3000, &v), (65535, 65535));
    }

    #[test]
    fn degenerate_extent_does_not_divide_by_zero() {
        let v = VirtualScreen {
            x: 0,
            y: 0,
            width: 1,
            height: 0,
        };
        assert_eq!(to_normalized_absolute(0, 0, &v), (0, 0));
    }

    #[test]
    fn layout_comparison_ignores_order_and_dpi_but_not_geometry() {
        let a = single_screen();
        let mut b = DesktopGeometry {
            monitors: vec![a.monitors[0].clone()],
        };
        b.monitors[0].dpi = 144;
        assert!(a.same_layout(&b), "仅 DPI 变化不应阻止执行");

        b.monitors[0].width = 2560;
        assert!(!a.same_layout(&b), "分辨率变化必须阻止执行");

        let two = DesktopGeometry {
            monitors: vec![
                MonitorInfo {
                    name: "B".into(),
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 100,
                    dpi: 96,
                },
                MonitorInfo {
                    name: "A".into(),
                    x: 100,
                    y: 0,
                    width: 100,
                    height: 100,
                    dpi: 96,
                },
            ],
        };
        let two_swapped = DesktopGeometry {
            monitors: vec![
                MonitorInfo {
                    name: "A".into(),
                    x: 100,
                    y: 0,
                    width: 100,
                    height: 100,
                    dpi: 96,
                },
                MonitorInfo {
                    name: "B".into(),
                    x: 0,
                    y: 0,
                    width: 100,
                    height: 100,
                    dpi: 96,
                },
            ],
        };
        assert!(two.same_layout(&two_swapped), "枚举顺序不同视为同一布局");
    }
}
