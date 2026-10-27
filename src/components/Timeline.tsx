import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { formatSeconds, formatTimelineTick } from "../format";
import type { ActionView, DocumentView } from "../types";
import { Icon } from "./Icon";
import {
    MAX_TIMELINE_ZOOM,
    MIN_TIMELINE_ZOOM,
    resizedWaitDuration,
    timelineTickTimes,
    zoomTimeline,
} from "../timeline";
import { recordingSegments } from "../timelineSegments";
import { clampTimelineLane, TIMELINE_HEIGHT_KEY, TIMELINE_LANE_DEFAULT, TimelineResizer } from "./TimelineResizer";
const RULER = 28,
    LANE_BASE = 44,
    RECORDING_HEIGHT = 36,
    WAIT_HEIGHT = 32,
    LANE_GAP = 10,
    PAD = 8,
    MUTED = "#616b70",
    ACCENT = "#1b6c80",
    RECORD = "#df5457",
    RIBBON_LINE = "#8eb5bf",
    RIBBON_BG = "#cde3e9",
    FIGURES = '"IBM Plex Sans","Segoe UI",sans-serif';
interface Props {
    hidden?: boolean;
    recording: DocumentView;
    rows: ActionView[];
    timeUs: number;
    selectedId: string | null;
    disabled?: boolean;
    onSeek: (timeUs: number, action?: ActionView) => void;
    onUpdateWait: (actionId: string, durationUs: number) => void;
}
function lane(action: ActionView) {
    return action.kind.type === "wait" ? 1 : 0;
}
function laneTop(index: number) {
    return RULER + index * (LANE_BASE + LANE_GAP);
}
function roundRect(ctx: CanvasRenderingContext2D, x: number, y: number, w: number, h: number, r: number) {
    const radius = Math.max(0, Math.min(r, w / 2, h / 2));
    ctx.beginPath();
    ctx.moveTo(x + radius, y);
    ctx.lineTo(x + w - radius, y);
    ctx.arcTo(x + w, y, x + w, y + radius, radius);
    ctx.lineTo(x + w, y + h - radius);
    ctx.arcTo(x + w, y + h, x + w - radius, y + h, radius);
    ctx.lineTo(x + radius, y + h);
    ctx.arcTo(x, y + h, x, y + h - radius, radius);
    ctx.lineTo(x, y + radius);
    ctx.arcTo(x, y, x + radius, y, radius);
    ctx.closePath();
}
export function Timeline(p: Props) {
    const trackRef = useRef<HTMLDivElement>(null);
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const [width, setWidth] = useState(600);
    const [view, setView] = useState({ zoom: 1, startUs: 0 });
    const [zoomSlot, setZoomSlot] = useState<HTMLElement | null>(null);
    const [zoomInput, setZoomInput] = useState<string | null>(null);
    const dragging = useRef<
        | { kind: "pan"; startX: number; startUs: number }
        | { kind: "seek" }
        | { kind: "resize"; startX: number; action: ActionView; edge: "left" | "right"; durationUs: number }
        | null
    >(null);
    const [resizePreview, setResizePreview] = useState<{ action: ActionView; durationUs: number } | null>(null);
    const [resizeHover, setResizeHover] = useState(false);
    const [headHover, setHeadHover] = useState(false);
    const displayRows = useMemo(() => {
        if (!resizePreview) return p.rows;
        const { action: wait, durationUs } = resizePreview;
        const delta = durationUs - (wait.endUs - wait.startUs);
        return p.rows.map(action => {
            if (action.id === wait.id) return { ...action, endUs: action.startUs + durationUs };
            if (action.startUs >= wait.endUs)
                return { ...action, startUs: action.startUs + delta, endUs: action.endUs + delta };
            return action;
        });
    }, [p.rows, resizePreview]);
    const displayDurationUs =
        p.recording.durationUs +
        (resizePreview ? resizePreview.durationUs - (resizePreview.action.endUs - resizePreview.action.startUs) : 0);
    const [laneHeight, setLaneHeight] = useState(() => {
        try {
            const saved = window.localStorage.getItem(TIMELINE_HEIGHT_KEY);
            return clampTimelineLane(saved === null ? TIMELINE_LANE_DEFAULT : Number(saved));
        } catch {
            return clampTimelineLane(TIMELINE_LANE_DEFAULT);
        }
    });
    const height = RULER + laneHeight * 2 + LANE_GAP;
    useEffect(() => {
        try {
            window.localStorage.setItem(TIMELINE_HEIGHT_KEY, String(laneHeight));
        } catch {
            /* storage unavailable */
        }
    }, [laneHeight]);
    useEffect(() => {
        const resize = () => setLaneHeight(current => clampTimelineLane(current));
        window.addEventListener("resize", resize);
        return () => window.removeEventListener("resize", resize);
    }, []);
    const extent = useMemo(
        () => Math.max(1_000_000, p.recording.durationUs + Math.max(100_000, p.recording.durationUs * 0.06)),
        [p.recording.durationUs],
    );
    const span = extent / view.zoom;
    const maxStart = Math.max(0, extent - span);
    const start = Math.max(0, Math.min(view.startUs, maxStart));
    useLayoutEffect(() => {
        setView({ zoom: 1, startUs: 0 });
    }, [p.recording.taskId]);
    useLayoutEffect(() => {
        setZoomSlot(p.hidden ? null : document.getElementById("timeline-zoom-slot"));
        setZoomInput(null);
    }, [p.hidden, p.recording.taskId]);
    useLayoutEffect(() => {
        const track = trackRef.current;
        if (!track) return;
        const measure = () => {
            const next = Math.max(1, track.getBoundingClientRect().width);
            setWidth(current => (current === next ? current : next));
        };
        measure();
        const observer = new ResizeObserver(measure);
        observer.observe(track);
        return () => observer.disconnect();
    }, []);
    // 从隐藏状态重新显示时，在浏览器绘制前重新测量并重绘，避免先露出旧画布再跳一下。
    useLayoutEffect(() => {
        if (p.hidden) return;
        const track = trackRef.current;
        if (!track) return;
        const next = Math.max(1, track.getBoundingClientRect().width);
        setWidth(current => (current === next ? current : next));
    }, [p.hidden]);
    const zoomAt = useCallback(
        (factor: number, x: number) =>
            setView(current => zoomTimeline(current, extent, Math.max(1, width - PAD * 2), factor, x - PAD)),
        [extent, width],
    );
    const commitZoomInput = () => {
        if (zoomInput !== null && zoomInput.trim() !== "") {
            const percent = Number(zoomInput);
            if (Number.isFinite(percent)) {
                setView(current =>
                    zoomTimeline(
                        current,
                        extent,
                        Math.max(1, width - PAD * 2),
                        percent / 100 / current.zoom,
                        width / 2 - PAD,
                    ),
                );
            }
        }
        setZoomInput(null);
    };
    useEffect(() => {
        const canvas = canvasRef.current;
        if (!canvas) return;
        const wheel = (event: WheelEvent) => {
            event.preventDefault();
            if (dragging.current?.kind === "resize") return;
            const rect = canvas.getBoundingClientRect();
            if (event.shiftKey)
                setView(current => ({
                    ...current,
                    startUs: Math.max(
                        0,
                        Math.min(
                            Math.max(0, extent - extent / current.zoom),
                            current.startUs + ((event.deltaY / width) * extent) / current.zoom,
                        ),
                    ),
                }));
            else zoomAt(event.deltaY < 0 ? 1.25 : 0.8, event.clientX - rect.left);
        };
        canvas.addEventListener("wheel", wheel, { passive: false });
        return () => canvas.removeEventListener("wheel", wheel);
    }, [extent, width, zoomAt]);
    useLayoutEffect(() => {
        const canvas = canvasRef.current,
            ctx = canvas?.getContext("2d");
        if (!canvas || !ctx) return;
        const dpr = window.devicePixelRatio || 1;
        const backingWidth = Math.round(width * dpr);
        const backingHeight = Math.round(height * dpr);
        // 只在尺寸变化时重置画布；切换任务时不要无谓清空重绘，避免闪帧。
        if (canvas.width !== backingWidth) canvas.width = backingWidth;
        if (canvas.height !== backingHeight) canvas.height = backingHeight;
        const scaleX = backingWidth / width,
            scaleY = backingHeight / height;
        ctx.setTransform(scaleX, 0, 0, scaleY, 0, 0);
        // 文字行起点与 1px 描边对齐到设备像素网格，否则 WebView2 会按亚像素摆放字形，
        // 笔画被摊进相邻像素而发糊。对齐基准是实际变换缩放（位图/CSS 尺寸），不是 dpr。
        const snapX = (v: number) => Math.round(v * scaleX) / scaleX;
        const snapY = (v: number) => Math.round(v * scaleY) / scaleY;
        const snapEdgeX = (v: number) => snapX(v) + 0.5 / scaleX;
        const snapEdgeY = (v: number) => snapY(v) + 0.5 / scaleY;
        ctx.clearRect(0, 0, width, height);
        ctx.fillStyle = "#fff";
        ctx.fillRect(0, 0, width, height);
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        const contentWidth = Math.max(1, width - PAD * 2);
        const xOf = (t: number) => PAD + ((t - start) / span) * contentWidth;
        const endX = xOf(displayDurationUs);
        // 刻度尺：mm:ss 整秒标签 + 圆点刻痕；亚秒只做无标签的中间点。
        const labelSteps = [1, 2, 5, 10, 15, 30, 60, 120, 300, 600, 900, 1800, 3600];
        let step = 3_600_000;
        for (const seconds of labelSteps) {
            const candidate = seconds * 1_000_000;
            if ((candidate / span) * contentWidth >= 72) {
                step = candidate;
                break;
            }
        }
        ctx.font = `12px ${FIGURES}`;
        for (const t of timelineTickTimes(start, span, displayDurationUs, step)) {
            const x = xOf(t);
            if (x < 0 || x > width) continue;
            const minor = Math.abs(t / step - Math.round(t / step)) > 0.01;
            ctx.fillStyle = minor ? "#e4e8ea" : "#cfd5d8";
            ctx.beginPath();
            ctx.arc(Math.max(1.2, Math.min(width - 1.2, x)), RULER - 5, minor ? 1.2 : 1.7, 0, Math.PI * 2);
            ctx.fill();
            if (!minor && !(x > 0 && endX > 0 && Math.abs(x - endX) < 48)) {
                ctx.fillStyle = MUTED;
                const label = formatTimelineTick(t);
                ctx.textAlign = "left";
                if (x < 24) {
                    ctx.fillText(label, snapX(2), snapY(11));
                } else if (x > width - 24) {
                    ctx.fillText(label, snapX(width - 2 - ctx.measureText(label).width), snapY(11));
                } else {
                    ctx.fillText(label, snapX(x - ctx.measureText(label).width / 2), snapY(11));
                }
            }
        }
        // 录制片段沿时间轴连续显示；插入的等待独占其时间区间。
        const waits = displayRows
            .filter(action => action.kind.type === "wait")
            .slice()
            .sort((a, b) => a.startUs - b.startUs);
        const segments = recordingSegments(displayRows, displayDurationUs);
        const selected = displayRows.find(action => action.id === p.selectedId && action.kind.type !== "wait");
        const top = snapEdgeY(laneTop(0) + (LANE_BASE - RECORDING_HEIGHT) / 2);
        for (const segment of segments) {
            const left = snapEdgeX(Math.max(PAD, xOf(segment.start)));
            const right = snapEdgeX(Math.min(width - PAD, xOf(segment.end)));
            if (right <= left) continue;
            ctx.fillStyle = RIBBON_BG;
            ctx.strokeStyle =
                selected && selected.startUs >= segment.start && selected.startUs < segment.end ? ACCENT : RIBBON_LINE;
            ctx.lineWidth = 1;
            roundRect(ctx, left, top, right - left, RECORDING_HEIGHT, 6);
            ctx.fill();
            ctx.stroke();
        }
        for (const action of waits) {
            const left = snapEdgeX(Math.max(PAD, xOf(action.startUs)));
            const right = snapEdgeX(Math.min(width - PAD, xOf(action.endUs)));
            if (right <= left) continue;
            const waitTop = snapEdgeY(laneTop(1) + (LANE_BASE - WAIT_HEIGHT) / 2);
            ctx.fillStyle = "#f5f1e8";
            ctx.strokeStyle = action.id === p.selectedId ? "#796342" : "#d8cdb7";
            ctx.lineWidth = 1;
            roundRect(ctx, left, waitTop, right - left, WAIT_HEIGHT, 5);
            ctx.fill();
            ctx.stroke();
            if (right - left > 64) {
                ctx.fillStyle = "#796342";
                ctx.font = `12px ${FIGURES}`;
                ctx.textAlign = "left";
                ctx.textBaseline = "middle";
                const text = formatSeconds(action.endUs - action.startUs);
                ctx.fillText(
                    text,
                    snapX((left + right) / 2 - ctx.measureText(text).width / 2),
                    snapY(waitTop + WAIT_HEIGHT / 2),
                );
                ctx.textBaseline = "alphabetic";
            }
        }
        const bottom = height;
        // 结束标记与播放头。
        if (displayDurationUs > 0 && endX >= 0 && endX <= width) {
            ctx.strokeStyle = RIBBON_LINE;
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(snapEdgeX(endX), RULER - 4);
            ctx.lineTo(snapEdgeX(endX), bottom);
            ctx.stroke();
            ctx.fillStyle = MUTED;
            ctx.font = `12px ${FIGURES}`;
            ctx.textAlign = "left";
            const endLabel = "结束";
            ctx.fillText(
                endLabel,
                endX > width - 34 ? snapX(endX - 4 - ctx.measureText(endLabel).width) : snapX(endX + 4),
                snapY(11),
            );
        }
        const head = xOf(p.timeUs);
        if (head >= 0 && head <= width) {
            ctx.strokeStyle = RECORD;
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(snapEdgeX(head), RULER - 9);
            ctx.lineTo(snapEdgeX(head), height - 1);
            ctx.stroke();
            ctx.fillStyle = RECORD;
            ctx.beginPath();
            ctx.arc(head, RULER - 9, 5, 0, Math.PI * 2);
            ctx.fill();
        }
    }, [
        displayRows,
        p.timeUs,
        p.selectedId,
        p.recording.taskId,
        displayDurationUs,
        width,
        height,
        laneHeight,
        view.zoom,
        start,
        span,
        p.hidden,
    ]);
    const hit = (clientX: number, clientY: number) => {
        const rect = canvasRef.current!.getBoundingClientRect();
        const x = clientX - rect.left;
        const y = clientY - rect.top;
        let rowLane = -1;
        if (y >= laneTop(0) && y < laneTop(0) + LANE_BASE) rowLane = 0;
        else if (y >= laneTop(1) && y < laneTop(1) + LANE_BASE) rowLane = 1;
        const time = start + ((x - PAD) / Math.max(1, width - PAD * 2)) * span;
        const tolerance = (span / Math.max(1, width - PAD * 2)) * 8;
        let best: ActionView | undefined;
        let distance = Infinity;
        for (const action of displayRows) {
            if (lane(action) !== rowLane) continue;
            const d = Math.min(
                Math.abs(action.startUs - time),
                time >= action.startUs && time <= action.endUs ? 0 : Infinity,
            );
            if (d < tolerance && d < distance) {
                best = action;
                distance = d;
            }
        }
        return { time: Math.max(0, Math.min(displayDurationUs, time)), action: best, x, y };
    };
    const waitEdgeAt = (clientX: number, clientY: number) => {
        const rect = canvasRef.current!.getBoundingClientRect();
        const x = clientX - rect.left;
        const y = clientY - rect.top;
        const top = laneTop(1) + (LANE_BASE - WAIT_HEIGHT) / 2;
        if (y < top || y > top + WAIT_HEIGHT) return null;
        let best: { action: ActionView; edge: "left" | "right"; distance: number } | null = null;
        for (const action of p.rows) {
            if (action.kind.type !== "wait" || !action.editable) continue;
            // 等待起点与前面的录制内容相接，只拖动结束边缘，避免手柄与指针反向移动。
            for (const edge of ["right"] as const) {
                const time = action.endUs;
                const edgeX = PAD + ((time - start) / span) * Math.max(1, width - PAD * 2);
                if (edgeX < PAD || edgeX > width - PAD) continue;
                const distance = Math.abs(x - edgeX);
                if (distance <= 7 && (!best || distance < best.distance)) best = { action, edge, distance };
            }
        }
        return best;
    };
    const isPlayheadAt = (clientX: number) => {
        const rect = canvasRef.current!.getBoundingClientRect();
        const headX = PAD + ((p.timeUs - start) / span) * Math.max(1, width - PAD * 2);
        return headX >= PAD && headX <= width - PAD && Math.abs(clientX - rect.left - headX) <= 7;
    };
    return (
        <section hidden={p.hidden} className="timeline" aria-label="录制时间线">
            <TimelineResizer laneHeight={laneHeight} onChange={setLaneHeight} />
            {zoomSlot &&
                createPortal(
                    <div className="timeline-zoom" aria-label="时间线缩放">
                        <button
                            type="button"
                            className="icon-button"
                            aria-label="缩小时间线"
                            title="缩小"
                            disabled={view.zoom <= MIN_TIMELINE_ZOOM}
                            onClick={() => zoomAt(0.8, width / 2)}
                        >
                            <Icon name="minus" size={14} />
                        </button>
                        {zoomInput === null ? (
                            <button
                                type="button"
                                className="zoom-value"
                                title="双击输入缩放比例（50%–400%）"
                                aria-label={`时间线缩放 ${Math.round(view.zoom * 100)}%，双击或按回车输入`}
                                onDoubleClick={() => setZoomInput(String(Math.round(view.zoom * 100)))}
                                onClick={event => {
                                    if (event.detail === 0) setZoomInput(String(Math.round(view.zoom * 100)));
                                }}
                            >
                                {Math.round(view.zoom * 100)}%
                            </button>
                        ) : (
                            <input
                                className="zoom-value zoom-input"
                                aria-label="输入时间线缩放比例，百分之五十到四百"
                                type="number"
                                min={MIN_TIMELINE_ZOOM * 100}
                                max={MAX_TIMELINE_ZOOM * 100}
                                value={zoomInput}
                                autoFocus
                                onFocus={event => event.currentTarget.select()}
                                onChange={event => setZoomInput(event.currentTarget.value)}
                                onBlur={commitZoomInput}
                                onKeyDown={event => {
                                    if (event.key === "Enter") event.currentTarget.blur();
                                    if (event.key === "Escape") setZoomInput(null);
                                }}
                            />
                        )}
                        <button
                            type="button"
                            className="icon-button"
                            aria-label="放大时间线"
                            title="放大"
                            disabled={view.zoom >= MAX_TIMELINE_ZOOM}
                            onClick={() => zoomAt(1.25, width / 2)}
                        >
                            <Icon name="plus" size={14} />
                        </button>
                    </div>,
                    zoomSlot,
                )}
            <div className="timeline-body">
                <div className="lane-labels">
                    <div className="ruler-label" style={{ height: RULER }} aria-hidden="true" />
                    <div className="lane-label" style={{ height: LANE_BASE }}>
                        <Icon name="mouse" size={15} />
                        录制
                    </div>
                    <div className="lane-label wait-label" style={{ height: LANE_BASE, marginTop: LANE_GAP }}>
                        <Icon name="clock" size={15} />
                        等待
                    </div>
                </div>
                <div className="timeline-track" ref={trackRef}>
                    <canvas
                        ref={canvasRef}
                        style={{ height, cursor: resizeHover ? "ew-resize" : headHover ? "grab" : undefined }}
                        onPointerDown={e => {
                            if (p.disabled) return;
                            if (e.button !== 0 && e.button !== 1) return;
                            e.currentTarget.setPointerCapture(e.pointerId);
                            const pan = e.shiftKey || e.button === 1;
                            const edge = !pan ? waitEdgeAt(e.clientX, e.clientY) : null;
                            if (edge) {
                                const durationUs = edge.action.endUs - edge.action.startUs;
                                dragging.current = {
                                    kind: "resize",
                                    startX: e.clientX,
                                    action: edge.action,
                                    edge: edge.edge,
                                    durationUs,
                                };
                                setResizePreview({ action: edge.action, durationUs });
                                setResizeHover(true);
                                p.onSeek(edge.action.startUs, edge.action);
                            } else if (pan) dragging.current = { kind: "pan", startX: e.clientX, startUs: start };
                            else {
                                dragging.current = { kind: "seek" };
                                const result = hit(e.clientX, e.clientY);
                                p.onSeek(result.time, result.action);
                            }
                        }}
                        onPointerMove={e => {
                            const drag = dragging.current;
                            if (!drag) {
                                const resize = !p.disabled && !!waitEdgeAt(e.clientX, e.clientY);
                                setResizeHover(resize);
                                setHeadHover(!p.disabled && !resize && isPlayheadAt(e.clientX));
                                return;
                            }
                            if (drag.kind === "resize") {
                                const original = drag.action.endUs - drag.action.startUs;
                                const durationUs = Math.min(
                                    Number.MAX_SAFE_INTEGER - p.recording.durationUs + original,
                                    resizedWaitDuration(
                                        original,
                                        e.clientX - drag.startX,
                                        span,
                                        width - PAD * 2,
                                        drag.edge,
                                    ),
                                );
                                drag.durationUs = durationUs;
                                setResizePreview({ action: drag.action, durationUs });
                            } else if (drag.kind === "pan")
                                setView(current => ({
                                    ...current,
                                    startUs: Math.max(
                                        0,
                                        Math.min(maxStart, drag.startUs - ((e.clientX - drag.startX) / width) * span),
                                    ),
                                }));
                            else {
                                const result = hit(e.clientX, e.clientY);
                                p.onSeek(result.time, result.action);
                            }
                        }}
                        onPointerUp={e => {
                            const drag = dragging.current;
                            dragging.current = null;
                            setResizePreview(null);
                            if (drag?.kind === "resize" && drag.durationUs !== drag.action.endUs - drag.action.startUs)
                                p.onUpdateWait(drag.action.id, drag.durationUs);
                            setResizeHover(!p.disabled && !!waitEdgeAt(e.clientX, e.clientY));
                            setHeadHover(!p.disabled && isPlayheadAt(e.clientX));
                            if (e.currentTarget.hasPointerCapture(e.pointerId))
                                e.currentTarget.releasePointerCapture(e.pointerId);
                        }}
                        onPointerCancel={() => {
                            dragging.current = null;
                            setResizePreview(null);
                            setResizeHover(false);
                            setHeadHover(false);
                        }}
                        onLostPointerCapture={() => {
                            dragging.current = null;
                            setResizePreview(null);
                            setResizeHover(false);
                            setHeadHover(false);
                        }}
                        onPointerLeave={() => {
                            if (!dragging.current) {
                                setResizeHover(false);
                                setHeadHover(false);
                            }
                        }}
                    />
                </div>
            </div>
        </section>
    );
}
