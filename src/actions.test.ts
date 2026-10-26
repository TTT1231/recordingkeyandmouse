import { describe, expect, it } from "vitest";
import { actionAtTime, desktopBounds } from "./actions";
import type { ActionView } from "./types";
const action: ActionView = {
    id: "move",
    startUs: 100_000,
    endUs: 300_000,
    kind: { type: "move", x: 100, y: 200 },
    title: "鼠标移动",
    detail: "",
    eventCount: 2,
    editable: true,
};
describe("共享预览状态", () => {
    it("原始停顿不会标成上一个动作仍在进行", () => {
        expect(actionAtTime([action], 50_000)).toBeNull();
        expect(actionAtTime([action], 200_000)?.id).toBe("move");
        expect(actionAtTime([action], 500_000)).toBeNull();
    });
    it("虚拟桌面保留负原点与不同尺寸显示器", () => {
        expect(
            desktopBounds([
                { x: -1280, y: 0, width: 1280, height: 1024 },
                { x: 0, y: -200, width: 1920, height: 1080 },
            ]),
        ).toEqual({ x: -1280, y: -200, width: 3200, height: 1224 });
    });
});
