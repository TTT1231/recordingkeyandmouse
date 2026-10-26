export interface TimelineView {
    zoom: number;
    startUs: number;
}
export function zoomTimeline(
    current: TimelineView,
    extent: number,
    width: number,
    factor: number,
    x: number,
): TimelineView {
    const zoom = Math.min(8, Math.max(1, current.zoom * factor));
    const oldSpan = extent / current.zoom;
    const oldStart = Math.max(0, Math.min(current.startUs, extent - oldSpan));
    const point = Math.max(0, Math.min(1, x / Math.max(1, width)));
    const focus = oldStart + point * oldSpan;
    const newSpan = extent / zoom;
    return { zoom, startUs: Math.max(0, Math.min(extent - newSpan, focus - point * newSpan)) };
}
