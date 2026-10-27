export interface TimelineView {
    zoom: number;
    startUs: number;
}
export const MIN_TIMELINE_ZOOM = 0.5;
export const MAX_TIMELINE_ZOOM = 4;

/** 刻度仅覆盖实际录制，视图缩小时结束后的空间不显示时间。 */
export function timelineTickTimes(startUs: number, spanUs: number, durationUs: number, stepUs: number): number[] {
    const end = Math.min(startUs + spanUs, Math.max(0, durationUs));
    const ticks: number[] = [];
    for (let time = Math.max(0, Math.floor(startUs / stepUs) * stepUs); time <= end; time += stepUs / 2)
        ticks.push(time);
    return ticks;
}

/** 将像素拖动转换为等待时长，等待起点固定，最短保留一个微秒。 */
export function resizedWaitDuration(
    originalUs: number,
    deltaPx: number,
    spanUs: number,
    contentWidth: number,
    edge: "left" | "right",
): number {
    const direction = edge === "right" ? 1 : -1;
    const deltaUs = Math.round((direction * deltaPx * spanUs) / Math.max(1, contentWidth));
    return Math.max(1, Math.min(Number.MAX_SAFE_INTEGER, originalUs + deltaUs));
}
export function zoomTimeline(
    current: TimelineView,
    extent: number,
    width: number,
    factor: number,
    x: number,
): TimelineView {
    const zoom = Math.min(MAX_TIMELINE_ZOOM, Math.max(MIN_TIMELINE_ZOOM, current.zoom * factor));
    const oldSpan = extent / current.zoom;
    const oldStart = Math.max(0, Math.min(current.startUs, Math.max(0, extent - oldSpan)));
    const point = Math.max(0, Math.min(1, x / Math.max(1, width)));
    const focus = oldStart + point * oldSpan;
    const newSpan = extent / zoom;
    return { zoom, startUs: Math.max(0, Math.min(Math.max(0, extent - newSpan), focus - point * newSpan)) };
}
