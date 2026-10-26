import { describe, expect, it } from "vitest";

import {
    INFINITE_ROUNDS,
    MAX_REPEAT_COUNT,
    parseRepeatCount,
    repeatModeLabel,
    repeatRounds,
    repeatSummary,
} from "./repeat";

describe("repeat modes", () => {
    it("maps once and infinite to execution rounds", () => {
        expect(repeatRounds("once", "10")).toBe(1);
        expect(repeatRounds("infinite", "10")).toBe(INFINITE_ROUNDS);
        expect(repeatRounds("count", "10")).toBe(10);
    });
    it("validates custom counts without silently clamping", () => {
        expect(parseRepeatCount("5")).toBe(5);
        expect(parseRepeatCount("1")).toBe(1);
        expect(parseRepeatCount(String(MAX_REPEAT_COUNT))).toBe(MAX_REPEAT_COUNT);
        expect(parseRepeatCount("")).toBeNull();
        expect(parseRepeatCount("0")).toBeNull();
        expect(parseRepeatCount("-3")).toBeNull();
        expect(parseRepeatCount("2.5")).toBeNull();
        expect(parseRepeatCount(String(MAX_REPEAT_COUNT + 1))).toBeNull();
        expect(repeatRounds("count", "abc")).toBeNull();
    });
    it("exposes a compact summary and an accessible label", () => {
        expect(repeatSummary("infinite", "10")).toBe("");
        expect(repeatSummary("once", "10")).toBe("1");
        expect(repeatSummary("count", "5")).toBe("5");
        expect(repeatModeLabel("infinite", "10")).toBe("无限循环");
        expect(repeatModeLabel("once", "10")).toBe("只执行一次");
        expect(repeatModeLabel("count", "5")).toBe("执行 5 次");
    });
});
