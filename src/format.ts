// 展示格式化：只描述事件本身的事实，不推测目标控件。

import type { EventKind, MouseButton } from "./types";

export const WHEEL_DELTA_UNIT = 120;

export function formatDurationUs(us: number): string {
    if (us < 0) return "0s";
    const totalMs = Math.floor(us / 1000);
    if (totalMs < 1000) return `${totalMs}ms`;
    const s = Math.floor(totalMs / 1000);
    if (s < 60) return `${(totalMs / 1000).toFixed(2)}s`;
    const m = Math.floor(s / 60);
    const restS = s % 60;
    if (m < 60) return `${m}分${restS}秒`;
    const h = Math.floor(m / 60);
    return `${h}时${m % 60}分${restS}秒`;
}

/** 原型时钟格式：mm:ss.mmm。 */
export function formatClock(us: number): string {
    const ms = Math.max(0, Math.floor(us / 1000));
    const m = Math.floor(ms / 60000);
    const s = Math.floor((ms % 60000) / 1000);
    const rest = ms % 1000;
    return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}.${String(rest).padStart(3, "0")}`;
}

/** 动作列表时间：原型使用 mm:ss.xx，不显示毫秒第三位。 */
export function formatShortClock(us: number): string {
    return formatClock(us).slice(0, -1);
}

/** 时间线刻度：mm:ss，按整秒标注；精确时间由播放头与动作列表提供。 */
export function formatTimelineTick(us: number): string {
    const total = Math.max(0, Math.floor(us / 1_000_000));
    const m = Math.floor(total / 60);
    const s = total % 60;
    return `${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

/** 任务列表中的时长：原型写作“6.8 秒”，最多保留两位小数。 */
export function formatSeconds(us: number): string {
    const seconds = Math.round((Math.max(0, us) / 1_000_000) * 100) / 100;
    return `${seconds} 秒`;
}

/** 执行中 Esc 始终可用，用户也可配置额外的停止键。 */
export function stopKeyNames(vk: number): string {
    return vk === 0x1b ? "Esc" : `Esc / ${vkName(vk)}`;
}

const VK_NAMES = new Map<number, string>([
    [0x08, "Backspace"],
    [0x09, "Tab"],
    [0x0d, "Enter"],
    [0x10, "Shift"],
    [0x11, "Ctrl"],
    [0x12, "Alt"],
    [0x13, "Pause"],
    [0x14, "CapsLock"],
    [0x1b, "Esc"],
    [0x20, "空格"],
    [0x21, "PageUp"],
    [0x22, "PageDown"],
    [0x23, "End"],
    [0x24, "Home"],
    [0x25, "←"],
    [0x26, "↑"],
    [0x27, "→"],
    [0x28, "↓"],
    [0x2d, "Insert"],
    [0x2e, "Delete"],
    [0x5b, "左Win"],
    [0x5c, "右Win"],
    [0x5d, "菜单"],
    [0x6a, "小键盘*"],
    [0x6b, "小键盘+"],
    [0x6d, "小键盘-"],
    [0x6e, "小键盘."],
    [0x6f, "小键盘/"],
    [0xa0, "左Shift"],
    [0xa1, "右Shift"],
    [0xa2, "左Ctrl"],
    [0xa3, "右Ctrl"],
    [0xa4, "左Alt"],
    [0xa5, "右Alt"],
    [0xba, ";"],
    [0xbb, "="],
    [0xbc, ","],
    [0xbd, "-"],
    [0xbe, "."],
    [0xbf, "/"],
    [0xc0, "`"],
    [0xdb, "["],
    [0xdc, "\\"],
    [0xdd, "]"],
    [0xde, "'"],
]);

export function vkName(vk: number): string {
    const known = VK_NAMES.get(vk);
    if (known) return known;
    if (vk >= 0x30 && vk <= 0x39) return String(vk - 0x30);
    if (vk >= 0x41 && vk <= 0x5a) return String.fromCharCode("A".charCodeAt(0) + (vk - 0x41));
    if (vk >= 0x60 && vk <= 0x69) return `小键盘${vk - 0x60}`;
    if (vk >= 0x70 && vk <= 0x87) return `F${vk - 0x6f}`;
    return `#0x${vk.toString(16).toUpperCase().padStart(2, "0")}`;
}

const BUTTON_NAMES: Record<MouseButton, string> = {
    left: "左键",
    right: "右键",
    middle: "中键",
    x1: "侧键1",
    x2: "侧键2",
};

export function buttonName(button: MouseButton): string {
    return BUTTON_NAMES[button];
}

export function wheelSteps(delta: number): string {
    const steps = Math.abs(delta) / WHEEL_DELTA_UNIT;
    return Number.isInteger(steps) ? String(steps) : steps.toFixed(1);
}

export function describeEvent(kind: EventKind): string {
    switch (kind.kind) {
        case "unicode":
            return "明确文本输入";
        case "keyDown":
            return `按下 ${vkName(kind.vk)}${kind.extended ? "（扩展键）" : ""}`;
        case "keyUp":
            return `抬起 ${vkName(kind.vk)}`;
        case "mouseMove":
            return `移动到 (${kind.x}, ${kind.y})`;
        case "mouseDown":
            return `按下${buttonName(kind.button)}`;
        case "mouseUp":
            return `抬起${buttonName(kind.button)}`;
        case "wheel": {
            const dir =
                kind.axis === "vertical" ? (kind.delta >= 0 ? "向上" : "向下") : kind.delta >= 0 ? "向右" : "向左";
            return `${kind.axis === "vertical" ? "垂直" : "水平"}滚轮 ${dir} ${wheelSteps(kind.delta)} 格`;
        }
    }
}

export function eventTypeName(kind: EventKind): string {
    switch (kind.kind) {
        case "unicode":
        case "keyDown":
        case "keyUp":
            return "键盘";
        case "mouseMove":
            return "移动";
        case "mouseDown":
        case "mouseUp":
            return "点击";
        case "wheel":
            return "滚轮";
    }
}
