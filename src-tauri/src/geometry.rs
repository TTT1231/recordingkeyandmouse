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
    /// 仅供执行副本使用：钩子可能在桌面外围报告最多 3 个物理像素的移动。
    /// 投影只允许虚拟桌面外围且落点属于唯一显示器，绝不跨显示器间隙猜测。
    pub(crate) fn project_exterior_move(&self, x: i32, y: i32) -> Option<(i32, i32)> {
        let mut bounds = self.monitors.iter().filter(|m| m.width > 0 && m.height > 0);
        let first = bounds.next()?;
        let (mut left, mut top) = (i64::from(first.x), i64::from(first.y));
        let (mut right, mut bottom) = (
            left + i64::from(first.width) - 1,
            top + i64::from(first.height) - 1,
        );
        for monitor in bounds {
            let mx = i64::from(monitor.x);
            let my = i64::from(monitor.y);
            left = left.min(mx);
            top = top.min(my);
            right = right.max(mx + i64::from(monitor.width) - 1);
            bottom = bottom.max(my + i64::from(monitor.height) - 1);
        }
        let (x, y) = (i64::from(x), i64::from(y));
        let (projected_x, projected_y) = (x.clamp(left, right), y.clamp(top, bottom));
        if (x == projected_x && y == projected_y)
            || (x - projected_x).abs() > 3
            || (y - projected_y).abs() > 3
        {
            return None;
        }
        let mut matches = self.monitors.iter().filter(|m| {
            m.width > 0
                && m.height > 0
                && projected_x >= i64::from(m.x)
                && projected_y >= i64::from(m.y)
                && projected_x < i64::from(m.x) + i64::from(m.width)
                && projected_y < i64::from(m.y) + i64::from(m.height)
        });
        matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        Some((
            i32::try_from(projected_x).ok()?,
            i32::try_from(projected_y).ok()?,
        ))
    }

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

    /// 执行前检查：布局与录制时不一致则阻止执行，不按比例猜测新目标。
    /// 只比较物理矩形与名称；DPI 值不参与此判断，不代表不同缩放环境已经通过原生验证。
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
    fn exterior_move_projection_is_small_unambiguous_and_overflow_safe() {
        let g = single_screen();
        for (point, expected) in [
            ((-3, 20), Some((0, 20))),
            ((1922, 20), Some((1919, 20))),
            ((20, -3), Some((20, 0))),
            ((20, 1082), Some((20, 1079))),
            ((-3, -3), Some((0, 0))),
            ((-4, 20), None),
            ((20, 20), None),
        ] {
            assert_eq!(g.project_exterior_move(point.0, point.1), expected);
        }
        let mut gap = g.clone();
        gap.monitors.push(MonitorInfo {
            name: "other".into(),
            x: 1922,
            y: 0,
            width: 500,
            height: 1080,
            dpi: 96,
        });
        assert_eq!(
            gap.project_exterior_move(1920, 20),
            None,
            "显示器间隙不能投影"
        );
        let mut overlap = g.clone();
        overlap.monitors.push(overlap.monitors[0].clone());
        assert_eq!(
            overlap.project_exterior_move(-1, 20),
            None,
            "有多个显示器落点时不能猜测"
        );
        let extreme = DesktopGeometry {
            monitors: vec![MonitorInfo {
                name: "edge".into(),
                x: i32::MAX - 1,
                y: -10,
                width: 5,
                height: 20,
                dpi: 96,
            }],
        };
        assert_eq!(
            extreme.project_exterior_move(i32::MAX - 2, 0),
            Some((i32::MAX - 1, 0))
        );
        assert_eq!(extreme.project_exterior_move(i32::MIN, 0), None);
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
