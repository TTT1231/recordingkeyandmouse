import type { MouseButton, RecordedEvent, WheelAxis } from "./types";
export interface ClickFlash {
    x: number;
    y: number;
    button: MouseButton;
    atUs: number;
}
export interface PreviewCursor {
    index: number;
    timeUs: number;
    hasPos: boolean;
    x: number;
    y: number;
    keysDown: Set<number>;
    buttonsDown: Set<MouseButton>;
    lastClick: ClickFlash | null;
    lastWheel: { x: number; y: number; axis: WheelAxis; delta: number; atUs: number } | null;
    textFeedback: string;
    textAtUs: number;
}
export function createCursor(): PreviewCursor {
    return {
        index: 0,
        timeUs: 0,
        hasPos: false,
        x: 0,
        y: 0,
        keysDown: new Set(),
        buttonsDown: new Set(),
        lastClick: null,
        lastWheel: null,
        textFeedback: "",
        textAtUs: 0,
    };
}
export function seek(cursor: PreviewCursor, events: RecordedEvent[], timeUs: number): void {
    if (timeUs < cursor.timeUs) Object.assign(cursor, createCursor());
    advance(cursor, events, timeUs);
}

/** Smooth sparse demo moves without changing recorded events or input state. */
export function previewPointerPosition(cursor: PreviewCursor, events: RecordedEvent[], smooth = false) {
    if (!cursor.hasPos) return null;
    const current = events[cursor.index - 1];
    const next = events[cursor.index];
    if (
        smooth &&
        current?.kind.kind === "mouseMove" &&
        next?.kind.kind === "mouseMove" &&
        next.timeUs > current.timeUs
    ) {
        const fraction = Math.max(0, Math.min(1, (cursor.timeUs - current.timeUs) / (next.timeUs - current.timeUs)));
        return {
            x: current.kind.x + (next.kind.x - current.kind.x) * fraction,
            y: current.kind.y + (next.kind.y - current.kind.y) * fraction,
        };
    }
    return { x: cursor.x, y: cursor.y };
}
export function advance(cursor: PreviewCursor, events: RecordedEvent[], timeUs: number): void {
    while (cursor.index < events.length && events[cursor.index].timeUs <= timeUs) {
        applyEvent(cursor, events[cursor.index]);
        cursor.index += 1;
    }
    cursor.timeUs = timeUs;
}
function applyEvent(cursor: PreviewCursor, event: RecordedEvent) {
    const k = event.kind;
    if ("x" in k) {
        cursor.x = k.x;
        cursor.y = k.y;
        cursor.hasPos = true;
    }
    switch (k.kind) {
        case "keyDown":
            cursor.keysDown.add(k.vk);
            break;
        case "keyUp":
            cursor.keysDown.delete(k.vk);
            break;
        case "mouseDown":
            cursor.buttonsDown.add(k.button);
            cursor.lastClick = { x: k.x, y: k.y, button: k.button, atUs: event.timeUs };
            break;
        case "mouseUp":
            cursor.buttonsDown.delete(k.button);
            break;
        case "wheel":
            cursor.lastWheel = { x: k.x, y: k.y, axis: k.axis, delta: k.delta, atUs: event.timeUs };
            break;
        case "unicode":
            if (!k.up) {
                if (cursor.textAtUs !== event.timeUs) cursor.textFeedback = "";
                cursor.textFeedback += String.fromCharCode(k.unit);
                cursor.textAtUs = event.timeUs;
            }
            break;
    }
}
