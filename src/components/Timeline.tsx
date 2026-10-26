import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { formatClock, formatSeconds, formatTimelineTick } from "../format";
import type { ActionView, DocumentView } from "../types";
import { Icon } from "./Icon";
import { zoomTimeline } from "../timeline";
const RULER = 28,
    LANE = 44,
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
    onEdit: (action: ActionView) => void;
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
function drawCursorMark(ctx: CanvasRenderingContext2D, x: number, y: number) {
    ctx.beginPath();
    ctx.moveTo(x, y);
    ctx.lineTo(x, y + 15);
    ctx.lineTo(x + 4, y + 11);
    ctx.lineTo(x + 7, y + 18);
    ctx.lineTo(x + 10, y + 16);
    ctx.lineTo(x + 7, y + 10);
    ctx.lineTo(x + 12, y + 9);
    ctx.closePath();
    ctx.fill();
}
function drawMouseMark(
    ctx: CanvasRenderingContext2D,
    cx: number,
    cy: number,
    button: string | null,
    doubleClick: boolean,
    wheel: boolean,
    color: string,
) {
    ctx.strokeStyle = color;
    ctx.fillStyle = color;
    ctx.lineWidth = 1.2;
    roundRect(ctx, cx - 8, cy - 10, 16, 20, 8);
    ctx.stroke();
    if (button === "left") ctx.fillRect(cx - 7, cy - 8, 6, 7);
    if (button === "right") ctx.fillRect(cx + 1, cy - 8, 6, 7);
    if (button === "middle") ctx.fillRect(cx - 1.5, cy - 8, 3, 7);
    if (wheel) {
        ctx.beginPath();
        ctx.moveTo(cx, cy - 6);
        ctx.lineTo(cx, cy + 4);
        ctx.moveTo(cx - 2.5, cy - 3.5);
        ctx.lineTo(cx, cy - 6);
        ctx.lineTo(cx + 2.5, cy - 3.5);
        ctx.moveTo(cx - 2.5, cy + 1.5);
        ctx.lineTo(cx, cy + 4);
        ctx.lineTo(cx + 2.5, cy + 1.5);
        ctx.stroke();
    }
    if (doubleClick) {
        ctx.beginPath();
        ctx.arc(cx + 11, cy - 4, 1.4, 0, Math.PI * 2);
        ctx.arc(cx + 11, cy, 1.4, 0, Math.PI * 2);
        ctx.fill();
    }
}
export function Timeline(p: Props) {
    const trackRef = useRef<HTMLDivElement>(null);
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const [width, setWidth] = useState(600);
    const [view, setView] = useState({ zoom: 1, startUs: 0 });
    const [hover, setHover] = useState<{ action: ActionView; x: number; y: number } | null>(null);
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
    useEffect(() => {
        setView({ zoom: 1, startUs: 0 });
    }, [p.recording.taskId]);
    useEffect(() => {
        const track = trackRef.current;
        if (!track) return;
        const observer = new ResizeObserver(() => setWidth(Math.max(1, track.clientWidth)));
        observer.observe(track);
        setWidth(Math.max(1, track.clientWidth));
        return () => observer.disconnect();
    }, []);
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
    useEffect(() => {
        const canvas = canvasRef.current,
            ctx = canvas?.getContext("2d");
        if (!canvas || !ctx) return;
        const dpr = window.devicePixelRatio || 1;
        canvas.width = Math.round(width * dpr);
        canvas.height = Math.round(height * dpr);
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.clearRect(0, 0, width, height);
        const xOf = (t: number) => ((t - start) / span) * width;
        const ribbonTop = RULER + 2,
            ribbonHeight = LANE * 2 - 4;
        const endX = xOf(p.recording.durationUs);
        // 刻度尺：整数标签 + 圆点刻痕（对齐原型 ruler-tick）。
        let step = 1_000_000;
        while ((step / span) * width < 66) step *= 2;
        while ((step / span) * width > 200 && step > 125_000) step /= 2;
        ctx.font = `12px ${FIGURES}`;
        for (let t = Math.floor(start / step) * step; t <= start + span; t += step / 2) {
            const x = xOf(t);
            if (x < 0 || x > width) continue;
            const minor = Math.abs(t / step - Math.round(t / step)) > 0.01;
            ctx.fillStyle = minor ? "#e4e8ea" : "#cfd5d8";
            ctx.beginPath();
            ctx.arc(x, RULER - 5, minor ? 1.2 : 1.7, 0, Math.PI * 2);
            ctx.fill();
            if (!minor && x > 22 && x < width - 22 && !(endX > 0 && Math.abs(x - endX) < 48)) {
                ctx.fillStyle = MUTED;
                ctx.textAlign = "center";
                ctx.fillText(formatTimelineTick(t), x, 11);
            }
        }
        // 录制 ribbon：一条连续圆角底板覆盖鼠标 + 键盘轨道。
        const ribbonRight = Math.min(width, endX);
        if (ribbonRight > 10) {
            ctx.fillStyle = RIBBON_BG;
            ctx.strokeStyle = RIBBON_LINE;
            ctx.lineWidth = 1;
            roundRect(ctx, 0.5, ribbonTop, ribbonRight - 0.5, ribbonHeight, 9);
            ctx.fill();
            ctx.stroke();
        }
        const bottom = height;
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
                if (emphasized) {
                    ctx.fillStyle = "#dcebef";
                    roundRect(ctx, left, top + 6, right - left, LANE - 12, 5);
                    ctx.fill();
                }
                ctx.fillStyle = ACCENT;
                drawCursorMark(ctx, Math.min(left + 8, right - 6), top + 12);
                ctx.fillStyle = emphasized ? ACCENT : ACCENT_LINE;
                roundRect(ctx, Math.min(left + 6, right - 6), top + 37, Math.max(12, right - left - 12), 2, 1);
                ctx.fill();
                if (action.kind.type === "drag") {
                    ctx.strokeStyle = "#52676c";
                    ctx.lineWidth = 1;
                    ctx.setLineDash([3, 2]);
                    roundRect(ctx, left, top + 6, right - left, LANE - 12, 5);
                    ctx.stroke();
                    ctx.setLineDash([]);
                }
                if (right - left >= 58) {
                    ctx.fillStyle = ACCENT;
                    ctx.font = `12px ${FIGURES}`;
                    ctx.textAlign = "left";
                    ctx.save();
                    ctx.beginPath();
                    ctx.rect(left, top, right - left, LANE);
                    ctx.clip();
                    ctx.fillText("移动", Math.min(left + 24, right - 34), top + 26);
                    ctx.restore();
                }
            } else if (action.kind.type === "click" || action.kind.type === "wheel") {
                const center = Math.max(12, Math.min(width - 12, x0));
                if (emphasized) {
                    ctx.fillStyle = ACCENT_SOFT;
                    ctx.strokeStyle = ACCENT;
                    ctx.lineWidth = 1;
                    roundRect(ctx, center - 12, top + 8, 24, 30, 4);
                    ctx.fill();
                    ctx.stroke();
                }
                drawMouseMark(
                    ctx,
                    center,
                    top + 23,
                    action.kind.type === "click" ? action.kind.button : null,
                    action.kind.type === "click" && action.kind.count === 2,
                    action.kind.type === "wheel",
                    ACCENT,
                );
                if (action.kind.type === "click") {
                    const index = p.rows.indexOf(action);
                    const next = p.rows.slice(index + 1).find(item => lane(item) === 0);
                    const room = (next ? xOf(next.startUs) : width) - center;
                    if (room > 72) {
                        const buttonLabel =
                            action.kind.button === "right"
                                ? "右键"
                                : action.kind.button === "middle"
                                  ? "中键"
                                  : action.kind.button === "x1"
                                    ? "侧键1"
                                    : action.kind.button === "x2"
                                      ? "侧键2"
                                      : "左键";
                        ctx.fillStyle = MUTED;
                        ctx.font = `12px ${FIGURES}`;
                        ctx.textAlign = "left";
                        ctx.fillText(
                            `${buttonLabel}${action.kind.count === 2 ? "双击" : "单击"}`,
                            center + 16,
                            top + 27,
                        );
                    }
                }
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
                const label = action.kind.type === "keys" ? action.kind.chord : action.kind.text.replace(/\n/g, "↵");
                const short = label.length > 12 ? `${label.slice(0, 11)}…` : label;
                ctx.font = `13px ${FIGURES}`;
                const left = Math.max(0, x0);
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
    }, [p.rows, p.timeUs, p.selectedId, p.recording.durationUs, width, height, view.zoom, start, span, hasWaits]);
    const hit = (clientX: number, clientY: number) => {
        const rect = canvasRef.current!.getBoundingClientRect();
        const x = clientX - rect.left;
        const y = clientY - rect.top;
        let rowLane = -1;
        if (y >= laneTop(0) && y < laneTop(0) + LANE) rowLane = 0;
        else if (y >= laneTop(1) && y < laneTop(1) + LANE) rowLane = 1;
        else if (hasWaits && y >= laneTop(2) && y < laneTop(2) + LANE) rowLane = 2;
        const time = start + (x / width) * span;
        const tolerance = (span / width) * 8;
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
                <h2>时间线</h2>
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
                    <div className="ruler-label">时间</div>
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
                        tabIndex={0}
                        role="slider"
                        aria-label="播放位置，方向键定位，Home 回到开头，End 到结尾"
                        aria-valuemin={0}
                        aria-valuemax={p.recording.durationUs / 1000}
                        aria-valuenow={p.timeUs / 1000}
                        aria-valuetext={formatClock(p.timeUs)}
                        onPointerDown={e => {
                            if (p.disabled) return;
                            e.currentTarget.setPointerCapture(e.pointerId);
                            dragging.current = { pan: e.shiftKey || e.button === 1, startX: e.clientX, startUs: start };
                            if (!dragging.current.pan) {
                                const result = hit(e.clientX, e.clientY);
                                p.onSeek(result.time, result.action);
                            }
                        }}
                        onPointerMove={e => {
                            const result = hit(e.clientX, e.clientY);
                            if (dragging.current) {
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
                                else p.onSeek(result.time, result.action);
                                setHover(null);
                            } else setHover(result.action ? { action: result.action, x: result.x, y: result.y } : null);
                        }}
                        onPointerUp={e => {
                            dragging.current = null;
                            if (e.currentTarget.hasPointerCapture(e.pointerId))
                                e.currentTarget.releasePointerCapture(e.pointerId);
                        }}
                        onPointerCancel={() => {
                            dragging.current = null;
                        }}
                        onPointerLeave={() => setHover(null)}
                        onDoubleClick={e => {
                            if (p.disabled) return;
                            const action = hit(e.clientX, e.clientY).action;
                            if (action) p.onEdit(action);
                        }}
                        onKeyDown={e => {
                            if (p.disabled) return;
                            if (["ArrowLeft", "ArrowRight", "Home", "End"].includes(e.key)) {
                                e.preventDefault();
                                p.onSeek(
                                    e.key === "Home"
                                        ? 0
                                        : e.key === "End"
                                          ? p.recording.durationUs
                                          : Math.max(
                                                0,
                                                Math.min(
                                                    p.recording.durationUs,
                                                    p.timeUs + (e.key === "ArrowLeft" ? -100_000 : 100_000),
                                                ),
                                            ),
                                );
                            }
                            if (e.key === "+" || e.key === "-") {
                                e.preventDefault();
                                zoomAt(e.key === "+" ? 1.25 : 0.8, width / 2);
                            }
                        }}
                    />
                    {hover && (
                        <div
                            className="timeline-tooltip"
                            role="tooltip"
                            style={{
                                left: Math.min(Math.max(4, hover.x), Math.max(4, width - 270)),
                                top: Math.max(20, hover.y - 44),
                            }}
                        >
                            <strong>{hover.action.title}</strong>
                            <span>{hover.action.detail}</span>
                            <span>
                                {formatClock(hover.action.startUs)} – {formatClock(hover.action.endUs)}
                            </span>
                        </div>
                    )}
                </div>
            </div>
        </section>
    );
}
