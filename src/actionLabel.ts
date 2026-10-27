// 动作行的展示文案：一行说清“做了什么”，参数只保留能帮助定位的那一个。
// 按钮与点击次数直接进入标题（左键单击 / 右键双击），不再用“单击（左键）”加一堆技术后缀。

import { buttonName, formatSeconds, wheelSteps } from "./format";
import type { ActionView, MouseButton } from "./types";

export type ActionValueTone = "coordinate" | "input" | "measure";

export interface ActionLabel {
    icon: string;
    title: string;
    value: string | null;
    tone: ActionValueTone;
}

const MOUSE_ICONS: Record<MouseButton, string> = {
    left: "mouse-left",
    right: "mouse-right",
    middle: "mouse-middle",
    x1: "mouse",
    x2: "mouse",
};

export function clickTitle(button: MouseButton, count: number): string {
    return `${buttonName(button)}${count >= 2 ? "双击" : "单击"}`;
}

export function pointText(x: number, y: number): string {
    return `${x}, ${y}`;
}

export function actionLabel(action: ActionView): ActionLabel {
    const kind = action.kind;
    switch (kind.type) {
        case "move":
            return { icon: "move", title: "鼠标移动", value: pointText(kind.x, kind.y), tone: "coordinate" };
        case "click":
            return {
                icon: MOUSE_ICONS[kind.button],
                title: clickTitle(kind.button, kind.count),
                value: pointText(kind.x, kind.y),
                tone: "coordinate",
            };
        case "drag":
            return {
                icon: "cursor",
                title: `${buttonName(kind.button)}拖拽`,
                value: pointText(kind.x, kind.y),
                tone: "coordinate",
            };
        case "wheel": {
            const forward = kind.delta >= 0;
            const title =
                kind.axis === "vertical" ? (forward ? "向上滚动" : "向下滚动") : forward ? "向右滚动" : "向左滚动";
            return {
                icon: kind.axis === "vertical" ? (forward ? "wheel-up" : "wheel-down") : "scroll",
                title,
                value: `${wheelSteps(kind.delta)} 格`,
                tone: "measure",
            };
        }
        case "keys":
            return { icon: "keyboard", title: "按下", value: kind.chord, tone: "input" };
        case "text":
            return { icon: "keyboard", title: "输入文本", value: kind.text.replace(/\n/g, "↵"), tone: "input" };
        case "wait":
            return {
                icon: "clock",
                title: "等待",
                value: formatSeconds(action.endUs - action.startUs),
                tone: "measure",
            };
    }
}
