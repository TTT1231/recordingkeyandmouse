import { describe, expect, it } from "vitest";
import { resizedWaitDuration } from "./timeline";

describe("拖动等待边缘", () => {
    it("左右边缘向外拉长、向内缩短，并按当前视图比例换算", () => {
        expect(resizedWaitDuration(1_000_000, 50, 10_000_000, 1000, "right")).toBe(1_500_000);
        expect(resizedWaitDuration(1_000_000, -50, 10_000_000, 1000, "left")).toBe(1_500_000);
        expect(resizedWaitDuration(1_000_000, -50, 10_000_000, 1000, "right")).toBe(500_000);
        expect(resizedWaitDuration(1_000_000, 50, 10_000_000, 1000, "left")).toBe(500_000);
    });
    it("等待时长保持正的整数微秒", () => {
        expect(resizedWaitDuration(10, -100, 1000, 100, "right")).toBe(1);
        expect(resizedWaitDuration(10, 1, 10, 3, "right")).toBe(13);
    });
});
