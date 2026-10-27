import { describe, expect, it } from "vitest";
import type { ActionView } from "./types";
import { recordingSegments } from "./timelineSegments";

const wait = (startUs: number, endUs: number): ActionView => ({
    id: `${startUs}`,
    startUs,
    endUs,
    kind: { type: "wait" },
    title: "等待",
    detail: "",
    eventCount: 0,
    editable: true,
});

describe("录制片段", () => {
    it("扣除插入的等待，保留其余全部录制时间", () => {
        expect(recordingSegments([wait(6, 8), wait(2, 3)], 10)).toEqual([
            { start: 0, end: 2 },
            { start: 3, end: 6 },
            { start: 8, end: 10 },
        ]);
    });
    it("处理首尾等待和重叠等待，不产生空片段", () => {
        expect(recordingSegments([wait(0, 2), wait(1, 4), wait(8, 10)], 10)).toEqual([{ start: 4, end: 8 }]);
        expect(recordingSegments([], 10)).toEqual([{ start: 0, end: 10 }]);
    });
});
