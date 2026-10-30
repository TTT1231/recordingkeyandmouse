import type { ScreenRecording } from "./types";

export interface ScreenTime {
    sourceTimeUs: number;
    rate: number;
    spanIndex: number;
}

/** Map an edited playhead to the original footage; inserted waits hold one frame. */
export function screenTimeAt(recording: ScreenRecording, timeUs: number): ScreenTime {
    const spans = recording.spans;
    const clampSource = (value: number) => Math.max(0, Math.min(recording.durationUs, value));
    if (!spans.length) return { sourceTimeUs: clampSource(timeUs), rate: 0, spanIndex: -1 };
    let low = 0;
    let high = spans.length;
    while (low < high) {
        const mid = Math.floor((low + high) / 2);
        if (spans[mid].endUs <= timeUs) low = mid + 1;
        else high = mid;
    }
    if (low === spans.length) {
        return { sourceTimeUs: clampSource(spans[low - 1].sourceEndUs), rate: 0, spanIndex: low - 1 };
    }
    const span = spans[low];
    if (timeUs < span.startUs) {
        return { sourceTimeUs: clampSource(span.sourceStartUs), rate: 0, spanIndex: low };
    }
    const duration = span.endUs - span.startUs;
    const rate = duration > 0 ? Math.max(0, (span.sourceEndUs - span.sourceStartUs) / duration) : 0;
    return {
        sourceTimeUs: clampSource(span.sourceStartUs + (timeUs - span.startUs) * rate),
        rate,
        spanIndex: low,
    };
}
