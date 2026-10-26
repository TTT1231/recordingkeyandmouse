import { describe, expect, it } from "vitest";

import { describeEvent, formatDurationUs, vkName, wheelSteps } from "./format";

describe("formatDurationUs", () => {
    it("短时间显示毫秒", () => {
        expect(formatDurationUs(0)).toBe("0ms");
        expect(formatDurationUs(450_000)).toBe("450ms");
    });

    it("秒级保留两位小数", () => {
        expect(formatDurationUs(1_234_567)).toBe("1.23s");
    });

    it("分钟级用中文单位", () => {
        expect(formatDurationUs(60_000_000)).toBe("1分0秒");
        expect(formatDurationUs(183_500_000)).toBe("3分3秒");
    });

    it("负数按 0 处理", () => {
        expect(formatDurationUs(-5)).toBe("0s");
    });
});

describe("vkName", () => {
    it("字母、数字与功能键", () => {
        expect(vkName(0x41)).toBe("A");
        expect(vkName(0x5a)).toBe("Z");
        expect(vkName(0x30)).toBe("0");
        expect(vkName(0x77)).toBe("F8");
        expect(vkName(0x09)).toBe("Tab");
    });

    it("未识别的显示原始码", () => {
        expect(vkName(0x1)).toBe("#0x01");
        expect(vkName(0x123)).toBe("#0x123");
    });
});

describe("describeEvent", () => {
    it("滚轮区分方向与轴，单位为格", () => {
        expect(describeEvent({ kind: "wheel", axis: "vertical", delta: -240, x: 200, y: 150 })).toContain("向下");
        expect(describeEvent({ kind: "wheel", axis: "vertical", delta: -240, x: 200, y: 150 })).toContain("2 格");
        expect(describeEvent({ kind: "wheel", axis: "horizontal", delta: 120, x: 200, y: 150 })).toContain("向右");
        expect(wheelSteps(360)).toBe("3");
    });

    it("鼠标按键使用中文命名", () => {
        expect(describeEvent({ kind: "mouseDown", button: "left", x: 200, y: 150 })).toContain("左键");
        expect(describeEvent({ kind: "mouseUp", button: "x2", x: 200, y: 150 })).toContain("侧键2");
    });
});
