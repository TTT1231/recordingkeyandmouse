import { describe, expect, it } from "vitest";

import type { RecordedEvent } from "./types";
import { advance, createCursor, seek } from "./preview";

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

    it("前进过程保留按住状态与轨迹", () => {
        const c = createCursor();
        advance(c, events, 1_200_000);
        expect(c.hasPos).toBe(true);
        expect(c.x).toBe(200);
        expect(c.y).toBe(150);
        expect([...c.keysDown]).toEqual([0x41]);
        expect(c.trail).toEqual([
            { x: 100, y: 100 },
            { x: 200, y: 150 },
        ]);
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
        expect(c.trail).toEqual([{ x: 100, y: 100 }]);
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
});
