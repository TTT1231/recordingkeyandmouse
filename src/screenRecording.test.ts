import { describe, expect, it } from "vitest";
import { screenTimeAt } from "./screenRecording";
import type { ScreenRecording } from "./types";

const recording = (spans: ScreenRecording["spans"], durationUs = 5_000_000): ScreenRecording => ({
    id: "fixture",
    width: 1920,
    height: 1080,
    originX: 0,
    originY: 0,
    durationUs,
    spans,
});

describe("录制画面时间映射", () => {
    const withWait = recording([
        { startUs: 0, endUs: 2_000_000, sourceStartUs: 0, sourceEndUs: 2_000_000 },
        { startUs: 2_000_000, endUs: 3_000_000, sourceStartUs: 2_000_000, sourceEndUs: 2_000_000 },
        { startUs: 3_000_000, endUs: 6_000_000, sourceStartUs: 2_000_000, sourceEndUs: 5_000_000 },
    ]);

    it("插入等待冻结画面，等待结束后继续原画面的时间", () => {
        expect(screenTimeAt(withWait, 2_500_000)).toEqual({ sourceTimeUs: 2_000_000, rate: 0, spanIndex: 1 });
        expect(screenTimeAt(withWait, 3_000_000)).toEqual({ sourceTimeUs: 2_000_000, rate: 1, spanIndex: 2 });
        expect(screenTimeAt(withWait, 4_000_000).sourceTimeUs).toBe(3_000_000);
    });

    it("反向拖动时重新定位源画面，不延用上一次所在片段", () => {
        expect(screenTimeAt(withWait, 5_000_000).sourceTimeUs).toBe(4_000_000);
        expect(screenTimeAt(withWait, 1_000_000)).toEqual({ sourceTimeUs: 1_000_000, rate: 1, spanIndex: 0 });
    });

    it("移动耗时改变后按比例播放原画面，后续停顿仍然保留", () => {
        const retimed = recording([
            { startUs: 0, endUs: 4_000_000, sourceStartUs: 0, sourceEndUs: 2_000_000 },
            { startUs: 4_000_000, endUs: 7_000_000, sourceStartUs: 2_000_000, sourceEndUs: 5_000_000 },
        ]);
        expect(screenTimeAt(retimed, 2_000_000)).toEqual({ sourceTimeUs: 1_000_000, rate: 0.5, spanIndex: 0 });
        expect(screenTimeAt(retimed, 6_500_000).sourceTimeUs).toBe(4_500_000);
        expect(screenTimeAt(retimed, 7_000_000)).toEqual({ sourceTimeUs: 5_000_000, rate: 0, spanIndex: 1 });
    });

    it("已裁掉时间段的边界选择右侧画面，开头与末尾越界不越过媒体范围", () => {
        const jump = recording([
            { startUs: 0, endUs: 1_000_000, sourceStartUs: 0, sourceEndUs: 1_000_000 },
            { startUs: 1_000_000, endUs: 3_000_000, sourceStartUs: 3_000_000, sourceEndUs: 5_000_000 },
        ]);
        expect(screenTimeAt(jump, 1_000_000).sourceTimeUs).toBe(3_000_000);
        expect(screenTimeAt(jump, -500_000)).toEqual({ sourceTimeUs: 0, rate: 0, spanIndex: 0 });
        expect(screenTimeAt(jump, 9_000_000).sourceTimeUs).toBe(5_000_000);
    });
});
