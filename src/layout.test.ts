import { describe, expect, it } from "vitest";

import { SIDEBAR_DEFAULT, SIDEBAR_MAX, SIDEBAR_MIN, clampSidebarWidth } from "./layout";

describe("clampSidebarWidth", () => {
    it("保留用户宽度并四舍五入", () => {
        expect(clampSidebarWidth(263.4)).toBe(263);
        expect(clampSidebarWidth(263.6)).toBe(264);
    });

    it("限制在最小与最大宽度之间", () => {
        expect(clampSidebarWidth(80)).toBe(SIDEBAR_MIN);
        expect(clampSidebarWidth(9999)).toBe(SIDEBAR_MAX);
    });

    it("窄窗口给工作区留出空间", () => {
        expect(clampSidebarWidth(400, 976)).toBe(400);
        expect(clampSidebarWidth(9999, 976)).toBe(416);
    });

    it("非法宽度回到默认值", () => {
        expect(clampSidebarWidth(Number.NaN)).toBe(SIDEBAR_DEFAULT);
        expect(clampSidebarWidth(Number.POSITIVE_INFINITY)).toBe(SIDEBAR_DEFAULT);
    });
});
