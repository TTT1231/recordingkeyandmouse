import { describe, expect, it } from "vitest";

import { actionLabel, clickTitle, pointText } from "./actionLabel";
import type { ActionKind, ActionView } from "./types";

function action(kind: ActionKind, startUs = 1_000_000, endUs = 1_080_000): ActionView {
    return {
        id: "a",
        startUs,
        endUs,
        kind,
        title: "",
        detail: "",
        eventCount: 1,
        editable: true,
    };
}

describe("actionLabel", () => {
    it("鼠标按键与点击次数直接写进标题", () => {
        expect(clickTitle("left", 1)).toBe("左键单击");
        expect(clickTitle("right", 2)).toBe("右键双击");
        expect(clickTitle("middle", 1)).toBe("中键单击");
        expect(actionLabel(action({ type: "click", button: "left", count: 1, x: 421, y: 473 }))).toMatchObject({
            icon: "mouse-left",
            title: "左键单击",
            value: "421, 473",
            tone: "coordinate",
        });
        expect(actionLabel(action({ type: "click", button: "right", count: 2, x: 942, y: 476 }))).toMatchObject({
            icon: "mouse-right",
            title: "右键双击",
        });
        expect(actionLabel(action({ type: "click", button: "middle", count: 1, x: 1, y: 2 })).icon).toBe(
            "mouse-middle",
        );
    });

    it("侧键保留真实名称，图标退回通用鼠标", () => {
        expect(actionLabel(action({ type: "click", button: "x2", count: 1, x: 1, y: 2 }))).toMatchObject({
            icon: "mouse",
            title: "侧键2单击",
        });
    });

    it("移动与拖拽只保留终点坐标，不再附带耗时文字", () => {
        expect(actionLabel(action({ type: "move", x: 421, y: 473 }))).toEqual({
            icon: "move",
            title: "鼠标移动",
            value: "421, 473",
            tone: "coordinate",
        });
        expect(actionLabel(action({ type: "drag", button: "left", x: 942, y: 476 }))).toMatchObject({
            icon: "cursor",
            title: "左键拖拽",
            value: "942, 476",
        });
    });

    it("滚轮区分方向与格数", () => {
        expect(actionLabel(action({ type: "wheel", axis: "vertical", delta: 360, x: 0, y: 0 }))).toMatchObject({
            icon: "wheel-up",
            title: "向上滚动",
            value: "3 格",
            tone: "measure",
        });
        expect(actionLabel(action({ type: "wheel", axis: "vertical", delta: -120, x: 0, y: 0 }))).toMatchObject({
            icon: "wheel-down",
            title: "向下滚动",
            value: "1 格",
        });
        expect(actionLabel(action({ type: "wheel", axis: "horizontal", delta: -240, x: 0, y: 0 }))).toMatchObject({
            icon: "scroll",
            title: "向左滚动",
            value: "2 格",
        });
    });

    it("键盘内容成为一行里的实际输入", () => {
        expect(actionLabel(action({ type: "keys", chord: "Ctrl + S" }))).toEqual({
            icon: "keyboard",
            title: "按下",
            value: "Ctrl + S",
            tone: "input",
        });
        expect(actionLabel(action({ type: "text", text: "b" }))).toMatchObject({ title: "输入文本", value: "b" });
        expect(actionLabel(action({ type: "text", text: "a\nb" })).value).toBe("a↵b");
    });

    it("等待使用时长本身作为参数", () => {
        expect(actionLabel(action({ type: "wait" }, 0, 800_000))).toEqual({
            icon: "clock",
            title: "等待",
            value: "0.8 秒",
            tone: "measure",
        });
    });

    it("坐标文本不加括号，减少重复符号", () => {
        expect(pointText(-12, 8)).toBe("-12, 8");
    });
});
