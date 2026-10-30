import { describe, expect, it } from "vitest";

import type { RecordedEvent } from "./types";
import { advance, createCursor, previewPointerPosition, seek } from "./preview";

function ev(seq: number, timeUs: number, kind: RecordedEvent["kind"]): RecordedEvent {
    return { seq, timeUs, nativeTimeMs: 0, kind };
}

describe("预览游标", () => {
    const events: RecordedEvent[] = [
        ev(1, 0, { kind: "mouseMove", x: 100, y: 100 }),
        ev(2, 500_000, { kind: "keyDown", vk: 0x41, scan: 0, extended: false }),
        ev(3, 1_000_000, { kind: "mouseMove", x: 200, y: 150 }),
        ev(4, 1_500_000, { kind: "keyUp", vk: 0x41, scan: 0, extended: false }),
        ev(5, 2_000_000, { kind: "mouseDown", button: "left", x: 200, y: 150 }),
        ev(6, 2_100_000, { kind: "mouseUp", button: "left", x: 200, y: 150 }),
    ];

    it("前进过程保留位置与按住状态", () => {
        const c = createCursor();
        advance(c, events, 1_200_000);
        expect(c.hasPos).toBe(true);
        expect(c.x).toBe(200);
        expect(c.y).toBe(150);
        expect([...c.keysDown]).toEqual([0x41]);
    });

    it("时间轴末尾后仍可继续推进（结尾停顿）", () => {
        const c = createCursor();
        advance(c, events, 10_000_000);
        expect(c.index).toBe(events.length);
        expect(c.timeUs).toBe(10_000_000);
        expect(c.keysDown.size).toBe(0);
        expect(c.buttonsDown.size).toBe(0);
    });

    it("回退时重建游标再前进", () => {
        const c = createCursor();
        advance(c, events, 2_000_000);
        expect(c.lastClick).not.toBeNull();
        seek(c, events, 600_000); // 回退到 A 按下之后、第二次移动之前
        expect(c.index).toBe(2); // 已消费：移动(0) + 按下 A(500k)
        expect([...c.keysDown]).toEqual([0x41]);
        expect(c.lastClick).toBeNull();
    });

    it("点击闪烁记录按下时刻与位置", () => {
        const c = createCursor();
        seek(c, events, 2_050_000);
        expect(c.lastClick).toEqual({ x: 200, y: 150, button: "left", atUs: 2_000_000 });
    });

    it("重复 seek 到相同时间不重复消费事件", () => {
        const c = createCursor();
        seek(c, events, 1_000_000);
        const indexAfterFirst = c.index;
        seek(c, events, 1_000_000);
        expect(c.index).toBe(indexAfterFirst);
    });

    it("演示在稀疏移动样本之间平滑显示，原始位置和事件保持不变", () => {
        const moves = [
            ev(1, 0, { kind: "mouseMove", x: 0, y: 20 }),
            ev(2, 100_000, { kind: "mouseMove", x: 100, y: 60 }),
        ];
        const c = createCursor();
        seek(c, moves, 25_000);
        expect(previewPointerPosition(c, moves, true)).toEqual({ x: 25, y: 30 });
        expect(previewPointerPosition(c, moves)).toEqual({ x: 0, y: 20 });
        expect(c.index).toBe(1);
        expect(moves[0].kind).toEqual({ kind: "mouseMove", x: 0, y: 20 });
        seek(c, moves, 100_000);
        expect(previewPointerPosition(c, moves, true)).toEqual({ x: 100, y: 60 });
        seek(c, moves, 50_000);
        expect(previewPointerPosition(c, moves, true)).toEqual({ x: 50, y: 40 });
    });

    it("平滑显示不越过点击或键盘事件，也不提前显示第一个位置", () => {
        const c = createCursor();
        expect(previewPointerPosition(c, events, true)).toBeNull();
        seek(c, events, 250_000);
        expect(previewPointerPosition(c, events, true)).toEqual({ x: 100, y: 100 });
        seek(c, events, 1_900_000);
        expect(previewPointerPosition(c, events, true)).toEqual({ x: 200, y: 150 });
        expect(c.buttonsDown.size).toBe(0);
        seek(c, events, 2_050_000);
        expect(c.buttonsDown.has("left")).toBe(true);
    });
});
