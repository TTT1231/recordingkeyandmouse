import type { ActionView } from "./types";

/** 录制内容在时间轴中扣除插入的等待后形成的连续片段。 */
export function recordingSegments(rows: ActionView[], durationUs: number): { start: number; end: number }[] {
    const waits = rows
        .filter(action => action.kind.type === "wait")
        .slice()
        .sort((a, b) => a.startUs - b.startUs);
    const segments: { start: number; end: number }[] = [];
    let segmentStart = 0;
    for (const wait of waits) {
        const waitStart = Math.max(0, Math.min(durationUs, wait.startUs));
        const waitEnd = Math.max(waitStart, Math.min(durationUs, wait.endUs));
        if (waitStart > segmentStart) segments.push({ start: segmentStart, end: waitStart });
        segmentStart = Math.max(segmentStart, waitEnd);
    }
    if (segmentStart < durationUs) segments.push({ start: segmentStart, end: durationUs });
    return segments;
}
