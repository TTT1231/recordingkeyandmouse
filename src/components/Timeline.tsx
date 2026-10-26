import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { formatSeconds, formatTimelineTick } from "../format";
import type { ActionView, DocumentView } from "../types";
import { Icon } from "./Icon";
import { zoomTimeline } from "../timeline";
const RULER = 28,
    LANE = 44,
    PAD = 8,
    MUTED = "#616b70",
    ACCENT = "#1b6c80",
    ACCENT_SOFT = "#e6f0f3",
    ACCENT_LINE = "#a7c0c9",
    RECORD = "#df5457",
    RIBBON_LINE = "#d9e3e7",
    RIBBON_BG = "#eff4f6",
    FIGURES = '"IBM Plex Sans","Segoe UI",sans-serif';
interface Props {
    hidden?: boolean;
    recording: DocumentView;
    rows: ActionView[];
    timeUs: number;
    selectedId: string | null;
    disabled?: boolean;
    onSeek: (timeUs: number, action?: ActionView) => void;
}
function lane(action: ActionView) {
    return action.kind.type === "wait" ? 2 : ["keys", "text"].includes(action.kind.type) ? 1 : 0;
}
function laneTop(index: number) {
    return RULER + index * LANE;
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
function drawSelectionBand(ctx: CanvasRenderingContext2D, x: number, top: number) {
    ctx.fillStyle = ACCENT_SOFT;
    roundRect(ctx, x - 5, top + 7, 10, 32, 4);
    ctx.fill();
}

/** 点击用一根竖向刻线表示：左键帽子在上、右键在下、中键居中，双击是双帽子。 */
function drawClickTick(
    ctx: CanvasRenderingContext2D,
    x: number,
    top: number,
    button: string,
    doubleClick: boolean,
    selected: boolean,
) {
    const y0 = top + 9,
        y1 = top + 37,
        center = (y0 + y1) / 2;
    if (selected) drawSelectionBand(ctx, x, top);
    ctx.strokeStyle = ACCENT;
    ctx.lineWidth = selected ? 2 : 1.4;
    ctx.beginPath();
    ctx.moveTo(x, y0);
    ctx.lineTo(x, y1);
    ctx.stroke();
    const capY = button === "right" ? y1 - 5 : button === "middle" ? center : y0 + 5;
    const gap = doubleClick ? 2.5 : 0;
    ctx.lineWidth = 1.6;
    ctx.beginPath();
    if (gap) {
        ctx.moveTo(x - 3.2, capY - gap);
        ctx.lineTo(x + 3.2, capY - gap);
        ctx.moveTo(x - 3.2, capY + gap);
        ctx.lineTo(x + 3.2, capY + gap);
    } else {
        ctx.moveTo(x - 3.2, capY);
        ctx.lineTo(x + 3.2, capY);
    }
    ctx.stroke();
}

/** 滚轮用同一条刻线加中空圆点，和点击的短帽子区分。 */
function drawWheelTick(ctx: CanvasRenderingContext2D, x: number, top: number, selected: boolean) {
    const y0 = top + 9,
        y1 = top + 37,
        center = (y0 + y1) / 2;
    if (selected) drawSelectionBand(ctx, x, top);
    ctx.strokeStyle = ACCENT;
    ctx.lineWidth = selected ? 2 : 1.4;
    ctx.beginPath();
    ctx.moveTo(x, y0);
    ctx.lineTo(x, y1);
    ctx.stroke();
    ctx.lineWidth = 1.6;
    ctx.beginPath();
    ctx.arc(x, center, 2.6, 0, Math.PI * 2);
    ctx.stroke();
}

/** 移动/拖拽用横线：拖拽为虚线并在终点画空心圆，避免每个动作重复一个大图标。 */
function drawMotionLine(
    ctx: CanvasRenderingContext2D,
    left: number,
    right: number,
    top: number,
    drag: boolean,
    selected: boolean,
) {
    const y = top + 25;
    if (selected) {
        ctx.fillStyle = ACCENT_SOFT;
        roundRect(ctx, left, top + 12, Math.max(4, right - left), 24, 5);
        ctx.fill();
    }
    ctx.strokeStyle = selected ? ACCENT : ACCENT_LINE;
    ctx.lineWidth = selected ? 2.4 : 2;
    ctx.setLineDash(drag ? [4, 3] : []);
    ctx.beginPath();
    ctx.moveTo(left + 1, y);
    ctx.lineTo(Math.max(left + 2, right - 1), y);
    ctx.stroke();
    ctx.setLineDash([]);
    if (drag) {
        // 拖拽只补一个终点短竖线：虚线本身表示按住拖动，避免密集时叠成一串圆点。
        ctx.strokeStyle = ACCENT;
        ctx.lineWidth = 1.4;
        ctx.beginPath();
        ctx.moveTo(right - 1, y - 4);
        ctx.lineTo(right - 1, y + 4);
        ctx.stroke();
    }
}
export function Timeline(p: Props) {
    const trackRef = useRef<HTMLDivElement>(null);
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const [width, setWidth] = useState(600);
    const [view, setView] = useState({ zoom: 1, startUs: 0 });
    const dragging = useRef<{ pan: boolean; startX: number; startUs: number } | null>(null);
    const hasWaits = useMemo(() => p.rows.some(a => a.kind.type === "wait"), [p.rows]);
    const height = RULER + LANE * (hasWaits ? 3 : 2);
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
        const track = trackRef.current;
        if (!track) return;
        const measure = () => {
            const next = Math.max(1, track.clientWidth);
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
        const next = Math.max(1, track.clientWidth);
        setWidth(current => (current === next ? current : next));
    }, [p.hidden]);
    const zoomAt = useCallback(
        (factor: number, x: number) => setView(current => zoomTimeline(current, extent, width, factor, x)),
        [extent, width],
    );
    useEffect(() => {
        const canvas = canvasRef.current;
        if (!canvas) return;
        const wheel = (event: WheelEvent) => {
            event.preventDefault();
            const rect = canvas.getBoundingClientRect();
            if (event.shiftKey)
                setView(current => ({
                    ...current,
                    startUs: Math.max(
                        0,
                        Math.min(
                            extent - extent / current.zoom,
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
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.clearRect(0, 0, width, height);
        ctx.fillStyle = "#fff";
        ctx.fillRect(0, 0, width, height);
        ctx.lineCap = "round";
        ctx.lineJoin = "round";
        const contentWidth = Math.max(1, width - PAD * 2);
        const xOf = (t: number) => PAD + ((t - start) / span) * contentWidth;
        const ribbonTop = RULER + 2,
            ribbonHeight = LANE * 2 - 4;
        const endX = xOf(p.recording.durationUs);
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
        for (let t = Math.floor(start / step) * step; t <= start + span; t += step / 2) {
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
                if (x < 24) {
                    ctx.textAlign = "left";
                    ctx.fillText(label, 2, 11);
                } else if (x > width - 24) {
                    ctx.textAlign = "right";
                    ctx.fillText(label, width - 2, 11);
                } else {
                    ctx.textAlign = "center";
                    ctx.fillText(label, x, 11);
                }
            }
        }
        // 录制 ribbon：一条连续圆角底板覆盖鼠标 + 键盘轨道。
        const ribbonRight = Math.min(width - PAD, endX);
        if (ribbonRight > PAD + 10) {
            ctx.fillStyle = RIBBON_BG;
            ctx.strokeStyle = RIBBON_LINE;
            ctx.lineWidth = 1;
            roundRect(ctx, PAD + 0.5, ribbonTop, ribbonRight - PAD - 0.5, ribbonHeight, 9);
            ctx.fill();
            ctx.stroke();
        }
        const bottom = height;
        let lastKeyRight = -1;
        for (const action of p.rows) {
            const x0 = xOf(action.startUs),
                x1 = xOf(action.endUs);
            if (x1 < -30 || x0 > width + 30) continue;
            const top = laneTop(lane(action));
            const selected = action.id === p.selectedId;
            const emphasized = selected;
            if (action.kind.type === "move" || action.kind.type === "drag") {
                const left = Math.max(0, x0),
                    right = Math.min(width, x1);
                if (right - left < 2) continue;
                drawMotionLine(ctx, left, right, top, action.kind.type === "drag", emphasized);
                if (right - left >= 74) {
                    ctx.fillStyle = ACCENT;
                    ctx.font = `12px ${FIGURES}`;
                    ctx.textAlign = "left";
                    ctx.save();
                    ctx.beginPath();
                    ctx.rect(left, top, right - left, LANE);
                    ctx.clip();
                    ctx.fillText(action.kind.type === "drag" ? "拖拽" : "移动", left + 10, top + 17);
                    ctx.restore();
                }
            } else if (action.kind.type === "click" || action.kind.type === "wheel") {
                const x = Math.max(1, Math.min(width - 1, x0));
                if (action.kind.type === "click")
                    drawClickTick(ctx, x, top, action.kind.button, action.kind.count === 2, emphasized);
                else drawWheelTick(ctx, x, top, emphasized);
            } else if (action.kind.type === "wait") {
                const left = Math.max(0, x0),
                    right = Math.min(width, Math.max(x1, x0 + 3));
                ctx.fillStyle = "#f5f1e8";
                ctx.strokeStyle = selected ? "#796342" : "#d8cdb7";
                ctx.lineWidth = 1;
                roundRect(ctx, left, top + 8, Math.max(3, right - left), LANE - 16, 5);
                ctx.fill();
                ctx.stroke();
                if (right - left > 64) {
                    ctx.fillStyle = "#796342";
                    ctx.font = `12px ${FIGURES}`;
                    ctx.textAlign = "center";
                    ctx.fillText(formatSeconds(action.endUs - action.startUs), (left + right) / 2, top + 26);
                }
            } else {
                const left = Math.max(0, x0);
                // 键盘动作密集时退化成短刻线，不再让白色芯片互相压住。
                if (left < lastKeyRight + 4) {
                    if (emphasized) {
                        ctx.fillStyle = ACCENT_SOFT;
                        roundRect(ctx, left - 2, top + 14, 6, 20, 3);
                        ctx.fill();
                    }
                    ctx.strokeStyle = emphasized ? ACCENT : "#4c5b63";
                    ctx.lineWidth = emphasized ? 2 : 1.2;
                    ctx.beginPath();
                    ctx.moveTo(left, top + 16);
                    ctx.lineTo(left, top + 32);
                    ctx.stroke();
                    lastKeyRight = Math.max(lastKeyRight, left + 3);
                    continue;
                }
                const label = action.kind.type === "keys" ? action.kind.chord : action.kind.text.replace(/\n/g, "↵");
                const short = label.length > 12 ? `${label.slice(0, 11)}…` : label;
                ctx.font = `13px ${FIGURES}`;
                const w = Math.max(24, Math.min(width - left, ctx.measureText(short).width + 16));
                const top2 = laneTop(1);
                ctx.fillStyle = emphasized ? "#fff" : "#ffffffc7";
                ctx.strokeStyle = emphasized ? ACCENT : "#d3dce1";
                ctx.lineWidth = 1;
                roundRect(ctx, left, top2 + 9, w, 26, 5);
                ctx.fill();
                ctx.stroke();
                ctx.fillStyle = emphasized ? ACCENT : "#4c5b63";
                ctx.textAlign = "left";
                ctx.save();
                ctx.beginPath();
                ctx.rect(left, top2 + 9, w, 26);
                ctx.clip();
                ctx.fillText(short, left + 8, top2 + 26);
                ctx.restore();
                lastKeyRight = left + w;
            }
        }
        // 结束标记与播放头。
        if (p.recording.durationUs > 0 && endX >= 0 && endX <= width) {
            ctx.strokeStyle = RIBBON_LINE;
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(endX + 0.5, RULER - 4);
            ctx.lineTo(endX + 0.5, bottom);
            ctx.stroke();
            ctx.fillStyle = MUTED;
            ctx.font = `12px ${FIGURES}`;
            ctx.textAlign = endX > width - 34 ? "right" : "left";
            ctx.fillText("结束", endX + (endX > width - 34 ? -4 : 4), 11);
        }
        const head = xOf(p.timeUs);
        if (head >= 0 && head <= width) {
            ctx.strokeStyle = RECORD;
            ctx.lineWidth = 1;
            ctx.beginPath();
            ctx.moveTo(head + 0.5, RULER - 9);
            ctx.lineTo(head + 0.5, height - 1);
            ctx.stroke();
            ctx.fillStyle = RECORD;
            ctx.beginPath();
            ctx.arc(head, RULER - 9, 5, 0, Math.PI * 2);
            ctx.fill();
        }
    }, [
        p.rows,
        p.timeUs,
        p.selectedId,
        p.recording.taskId,
        p.recording.durationUs,
        width,
        height,
        view.zoom,
        start,
        span,
        hasWaits,
        p.hidden,
    ]);
    const hit = (clientX: number, clientY: number) => {
        const rect = canvasRef.current!.getBoundingClientRect();
        const x = clientX - rect.left;
        const y = clientY - rect.top;
        let rowLane = -1;
        if (y >= laneTop(0) && y < laneTop(0) + LANE) rowLane = 0;
        else if (y >= laneTop(1) && y < laneTop(1) + LANE) rowLane = 1;
        else if (hasWaits && y >= laneTop(2) && y < laneTop(2) + LANE) rowLane = 2;
        const time = start + ((x - PAD) / Math.max(1, width - PAD * 2)) * span;
        const tolerance = (span / Math.max(1, width - PAD * 2)) * 8;
        let best: ActionView | undefined;
        let distance = Infinity;
        for (const action of p.rows) {
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
        return { time: Math.max(0, Math.min(p.recording.durationUs, time)), action: best, x, y };
    };
    return (
        <section hidden={p.hidden} className="timeline" aria-label="录制时间线" data-tour="timeline">
            <div className="timeline-heading">
                <div className="timeline-zoom">
                    <button
                        type="button"
                        className="icon-button"
                        aria-label="缩小时间线"
                        title="缩小"
                        disabled={view.zoom <= 1}
                        onClick={() => zoomAt(0.8, width / 2)}
                    >
                        <Icon name="minus" size={14} />
                    </button>
                    <button
                        type="button"
                        className="zoom-fit-label"
                        title="点击显示完整时间线"
                        aria-label="适合窗口，显示完整时间线"
                        onClick={() => setView({ zoom: 1, startUs: 0 })}
                    >
                        {view.zoom === 1 ? "适合窗口" : `${Math.round(view.zoom * 100)}%`}
                    </button>
                    <button
                        type="button"
                        className="icon-button"
                        aria-label="放大时间线"
                        title="放大"
                        disabled={view.zoom >= 8}
                        onClick={() => zoomAt(1.25, width / 2)}
                    >
                        <Icon name="plus" size={14} />
                    </button>
                    <button
                        type="button"
                        className="icon-button"
                        aria-label="时间线适合窗口"
                        title="适合窗口"
                        aria-pressed={view.zoom === 1}
                        onClick={() => setView({ zoom: 1, startUs: 0 })}
                    >
                        <Icon name="maximize" size={14} />
                    </button>
                </div>
            </div>
            <div className="timeline-body">
                <div className="lane-labels">
                    <div className="ruler-label" aria-hidden="true" />
                    <div className="lane-label">
                        <Icon name="mouse" size={15} />
                        鼠标
                    </div>
                    <div className="lane-label">
                        <Icon name="keyboard" size={15} />
                        键盘
                    </div>
                    {hasWaits && (
                        <div className="lane-label wait-label">
                            <Icon name="clock" size={15} />
                            等待
                        </div>
                    )}
                </div>
                <div className="timeline-track" ref={trackRef}>
                    <canvas
                        ref={canvasRef}
                        style={{ height }}
                        onPointerDown={e => {
                            if (p.disabled) return;
                            e.currentTarget.setPointerCapture(e.pointerId);
                            const pan = e.shiftKey || e.button === 1;
                            dragging.current = { pan, startX: e.clientX, startUs: start };
                            if (!pan) {
                                const result = hit(e.clientX, e.clientY);
                                p.onSeek(result.time, result.action);
                            }
                        }}
                        onPointerMove={e => {
                            if (!dragging.current) return;
                            if (dragging.current.pan)
                                setView(current => ({
                                    ...current,
                                    startUs: Math.max(
                                        0,
                                        Math.min(
                                            maxStart,
                                            dragging.current!.startUs -
                                                ((e.clientX - dragging.current!.startX) / width) * span,
                                        ),
                                    ),
                                }));
                            else {
                                const result = hit(e.clientX, e.clientY);
                                p.onSeek(result.time, result.action);
                            }
                        }}
                        onPointerUp={e => {
                            dragging.current = null;
                            if (e.currentTarget.hasPointerCapture(e.pointerId))
                                e.currentTarget.releasePointerCapture(e.pointerId);
                        }}
                        onPointerCancel={() => {
                            dragging.current = null;
                        }}
                    />
                </div>
            </div>
        </section>
    );
}
