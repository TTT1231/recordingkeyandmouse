import { describe, expect, it } from "vitest";
import { zoomTimeline } from "./timeline";
describe("时间线缩放", () => {
    it("放大与缩小都保持鼠标指向的时间", () => {
        const original = { zoom: 4, startUs: 20_000_000 };
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
        expect(zoomTimeline({ zoom: 1, startUs: 0 }, 10_000_000, 500, 0.8, 500)).toEqual({ zoom: 1, startUs: 0 });
        const right = zoomTimeline({ zoom: 64, startUs: 9_000_000 }, 10_000_000, 500, 2, 600);
        expect(right.zoom).toBe(64);
        expect(right.startUs).toBeLessThanOrEqual(10_000_000 - 10_000_000 / 64);
    });
});
