import { describe, expect, it } from "vitest";
import { timelineTickTimes, zoomTimeline } from "./timeline";
describe("时间线缩放", () => {
    it("放大与缩小都保持鼠标指向的时间", () => {
        const original = { zoom: 2, startUs: 20_000_000 };
        const extent = 180_000_000,
            width = 1000,
            x = 400;
        const focus = original.startUs + ((extent / original.zoom) * x) / width;
        const larger = zoomTimeline(original, extent, width, 1.25, x);
        expect(larger.startUs + ((extent / larger.zoom) * x) / width).toBeCloseTo(focus);
        const restored = zoomTimeline(larger, extent, width, 0.8, x);
        expect(restored.zoom).toBe(original.zoom);
        expect(restored.startUs).toBeCloseTo(original.startUs);
    });
    it("缩放上下限与边缘保持在完整时长内", () => {
        expect(zoomTimeline({ zoom: 0.5, startUs: 0 }, 10_000_000, 500, 0.8, 500)).toEqual({ zoom: 0.5, startUs: 0 });
        const right = zoomTimeline({ zoom: 4, startUs: 9_000_000 }, 10_000_000, 500, 2, 600);
        expect(right.zoom).toBe(4);
        expect(right.startUs).toBeLessThanOrEqual(10_000_000 - 10_000_000 / 4);
    });
    it("录制进行中范围增长时视图收敛到有效区间", () => {
        // 已放大并平移到末尾后，录制继续使总时长（extent）增长：
        // 视图必须保持有效（不越界），且缩放倍率不被重置。
        let view = zoomTimeline({ zoom: 1, startUs: 0 }, 60_000_000, 1000, 4, 900);
        view = zoomTimeline(view, 60_000_000, 1000, 1.1, 500);
        for (const extent of [90_000_000, 130_000_000, 180_000_000]) {
            view = zoomTimeline(view, extent, 1000, 1, 500);
            const span = extent / view.zoom;
            expect(view.startUs).toBeGreaterThanOrEqual(0);
            expect(view.startUs).toBeLessThanOrEqual(Math.max(0, extent - span));
            expect(view.startUs + span).toBeLessThanOrEqual(extent);
        }
        expect(view.zoom).toBeGreaterThan(1);
    });
});

describe("时间线刻度", () => {
    it("缩小到半屏时不显示录制结束后的刻度", () => {
        expect(timelineTickTimes(0, 8_000_000, 3_800_000, 1_000_000)).toEqual([
            0, 500_000, 1_000_000, 1_500_000, 2_000_000, 2_500_000, 3_000_000, 3_500_000,
        ]);
    });
    it("平移或调整等待后，刻度随可见区间与新时长变化", () => {
        expect(timelineTickTimes(2_200_000, 2_000_000, 3_000_000, 1_000_000)).toEqual([
            2_000_000, 2_500_000, 3_000_000,
        ]);
        expect(timelineTickTimes(2_200_000, 2_000_000, 4_000_000, 1_000_000)).toEqual([
            2_000_000, 2_500_000, 3_000_000, 3_500_000, 4_000_000,
        ]);
    });
});
