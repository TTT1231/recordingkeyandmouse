import { describe, expect, it } from "vitest";
import { buildEditRequest, type EditorFormValues } from "./editor";
import type { ActionView } from "./types";

function action(kind: ActionView["kind"]): ActionView {
    return {
        id: "a1",
        startUs: 0,
        endUs: 100_000,
        kind,
        title: "t",
        detail: "",
        eventCount: 1,
        editable: true,
    };
}
const base: EditorFormValues = {
    x: "10",
    y: "20",
    duration: "1",
    button: "left",
    count: 1,
    steps: "1",
    direction: 1,
    text: "你好",
    chord: "Ctrl + S",
    linked: false,
};
describe("编辑请求构造", () => {
    it("等待：插入与修改共用正数秒校验", () => {
        expect(buildEditRequest({ insert: "wait", after: "a0", linkAvailable: false }, base)).toEqual({
            edit: { op: "insertWait", afterAction: "a0", durationUs: 1_000_000 },
        });
        expect(
            buildEditRequest(
                { action: action({ type: "wait" }), after: null, linkAvailable: false },
                { ...base, duration: "0.5" },
            ),
        ).toEqual({ edit: { op: "updateWait", actionId: "a1", durationUs: 500_000 } });
        for (const bad of ["0", "-1", "abc", "1e30"]) {
            const result = buildEditRequest(
                { insert: "wait", after: null, linkAvailable: false },
                { ...base, duration: bad },
            );
            expect("error" in result && result.error).toBe("等待时长必须大于零");
        }
    });
    it("移动：整数坐标与非负耗时可提交，联动状态原样传递", () => {
        expect(
            buildEditRequest(
                { action: action({ type: "move", x: 1, y: 2 }), after: null, linkAvailable: true },
                { ...base, duration: "0.2505" },
            ),
        ).toEqual({
            edit: { op: "updateMove", actionId: "a1", x: 10, y: 20, durationUs: 250_500, linkNextClick: false },
        });
        expect(
            buildEditRequest(
                { action: action({ type: "move", x: 1, y: 2 }), after: null, linkAvailable: true },
                { ...base, duration: "0", linked: true },
            ),
        ).toEqual({ edit: { op: "updateMove", actionId: "a1", x: 10, y: 20, durationUs: 0, linkNextClick: true } });
        expect(
            buildEditRequest(
                { action: action({ type: "move", x: 1, y: 2 }), after: null, linkAvailable: true },
                { ...base, duration: "-5" },
            ),
        ).toEqual({ error: "请填写有效的整数坐标和非负耗时" });
        expect(
            buildEditRequest(
                { action: action({ type: "move", x: 1, y: 2 }), after: null, linkAvailable: true },
                { ...base, x: "1.5" },
            ),
        ).toEqual({ error: "请填写有效的整数坐标和非负耗时" });
    });
    it("点击：按键与次数来自表单，坐标必须为整数", () => {
        expect(
            buildEditRequest(
                {
                    action: action({ type: "click", button: "left", count: 1, x: 0, y: 0 }),
                    after: null,
                    linkAvailable: false,
                },
                { ...base, button: "right", count: 2, x: "-5", y: "30" },
            ),
        ).toEqual({ edit: { op: "updateClick", actionId: "a1", x: -5, y: 30, button: "right", count: 2 } });
        expect(
            buildEditRequest(
                {
                    action: action({ type: "click", button: "left", count: 1, x: 0, y: 0 }),
                    after: null,
                    linkAvailable: false,
                },
                { ...base, y: "20.5" },
            ),
        ).toEqual({ error: "坐标应为整数" });
    });
    it("滚轮：方向乘格数换算原生增量，支持小数格，零报错", () => {
        expect(
            buildEditRequest(
                {
                    action: action({ type: "wheel", axis: "vertical", delta: 120, x: 0, y: 0 }),
                    after: null,
                    linkAvailable: false,
                },
                { ...base, steps: "1.5", direction: -1 },
            ),
        ).toEqual({ edit: { op: "updateWheel", actionId: "a1", delta: -180 } });
        for (const bad of [{ steps: "0" }, { steps: "abc" }]) {
            const result = buildEditRequest(
                {
                    action: action({ type: "wheel", axis: "vertical", delta: 120, x: 0, y: 0 }),
                    after: null,
                    linkAvailable: false,
                },
                { ...base, ...bad },
            );
            expect("error" in result && result.error).toBe("滚动量必须大于零");
        }
    });
    it("键盘与文本：按动作来源分流到按键或明确文本", () => {
        expect(
            buildEditRequest({ action: action({ type: "keys", chord: "A" }), after: null, linkAvailable: false }, base),
        ).toEqual({ edit: { op: "updateKeys", actionId: "a1", chord: "Ctrl + S" } });
        expect(
            buildEditRequest({ action: action({ type: "text", text: "旧" }), after: null, linkAvailable: false }, base),
        ).toEqual({ edit: { op: "updateText", actionId: "a1", text: "你好" } });
        expect(buildEditRequest({ insert: "text", after: "a0", linkAvailable: false }, base)).toEqual({
            edit: { op: "insertText", afterAction: "a0", text: "你好" },
        });
    });
    it("不可参数编辑的动作返回错误而不是构造请求", () => {
        expect(
            buildEditRequest(
                {
                    action: { ...action({ type: "drag", button: "left", x: 0, y: 0 }), editable: false },
                    after: null,
                    linkAvailable: false,
                },
                base,
            ),
        ).toEqual({ error: "此动作暂不支持参数编辑" });
        expect(
            buildEditRequest(
                {
                    action: {
                        ...action({ type: "wheel", axis: "horizontal", delta: 120, x: 0, y: 0 }),
                        editable: false,
                    },
                    after: null,
                    linkAvailable: false,
                },
                base,
            ),
        ).toEqual({
            error: "此动作暂不支持参数编辑",
        });
    });
});
