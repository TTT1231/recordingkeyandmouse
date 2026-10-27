import { describe, expect, it } from "vitest";
import { clampTimelineLane, maxTimelineLane, TIMELINE_LANE_DEFAULT } from "./TimelineResizer";

describe("时间线高度范围", () => {
    it("限制常规窗口中的最小与最大高度", () => {
        expect(clampTimelineLane(-100, 1000)).toBe(44);
        expect(clampTimelineLane(1000, 1000)).toBe(100);
        expect(clampTimelineLane(72.4, 1000)).toBe(72);
        expect(clampTimelineLane(Number.NaN, 1000)).toBe(TIMELINE_LANE_DEFAULT);
    });

    it("窗口较小时保留上方空间，同时保证轨道最小高度", () => {
        expect(maxTimelineLane(680)).toBe(72);
        expect(clampTimelineLane(100, 680)).toBe(72);
        expect(maxTimelineLane(500)).toBe(44);
        expect(clampTimelineLane(100, 500)).toBe(44);
    });
});
