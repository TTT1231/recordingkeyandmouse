import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { formatClock } from "../format";
import type { ActionView, DocumentView } from "../types";
import { Icon } from "./Icon";
import { zoomTimeline } from "../timeline";
const RULER = 26,
    LANE = 28,
    GAP = 7,
    HEIGHT = RULER + (LANE + GAP) * 3;
interface Props {
    hidden?: boolean;
    recording: DocumentView;
    rows: ActionView[];
    timeUs: number;
    selectedId: string | null;
    activeId: string | null;
    disabled?: boolean;
    onSeek: (timeUs: number, action?: ActionView) => void;
    onEdit: (action: ActionView) => void;
}
function lane(action: ActionView) {
    return action.kind.type === "wait" ? 2 : ["keys", "text"].includes(action.kind.type) ? 1 : 0;
}
export function Timeline(p: Props) {
    const trackRef = useRef<HTMLDivElement>(null);
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const [width, setWidth] = useState(600);
    const [view, setView] = useState({ zoom: 1, startUs: 0 });
    const [hover, setHover] = useState<{ action: ActionView; x: number; y: number } | null>(null);
    const dragging = useRef<{ pan: boolean; startX: number; startUs: number } | null>(null);
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
        canvas.height = HEIGHT * dpr;
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.clearRect(0, 0, width, HEIGHT);
        const xOf = (t: number) => ((t - start) / span) * width;
        ctx.font = "11px system-ui, sans-serif";
        ctx.fillStyle = "#616b70";
        ctx.strokeStyle = "#d5dadd";
        ctx.lineWidth = 1;
        let step = 1_000_000;
        while ((step / span) * width < 75) step *= 2;
        while ((step / span) * width > 200 && step > 10_000) step /= 2;
        const endX = xOf(p.recording.durationUs);
        for (let t = Math.floor(start / step) * step; t <= start + span; t += step / 2) {
            const x = xOf(t);
            if (x < 0 || x > width) continue;
            const minor = Math.abs(t / step - Math.round(t / step)) > 0.01;
            ctx.beginPath();
            ctx.moveTo(x + 0.5, RULER - (minor ? 4 : 8));
            ctx.lineTo(x + 0.5, RULER);
            ctx.stroke();
            if (!minor && x < width - 48 && !(p.recording.durationUs > 0 && Math.abs(x - endX) < 45))
                ctx.fillText(formatClock(t).slice(0, 8), x + 3, 11);
        }
        for (let i = 0; i < 3; i++) {
            const y = RULER + GAP + i * (LANE + GAP);
            ctx.fillStyle = "#f5f6f7";
            ctx.fillRect(0, y, width, LANE);
            ctx.fillStyle = "#edf0f1";
            ctx.fillRect(0, y, Math.max(0, Math.min(width, endX)), LANE);
        }
        for (const action of p.rows) {
            const x0 = xOf(action.startUs),
                x1 = xOf(action.endUs);
            if (x1 < -8 || x0 > width + 8) continue;
            const y = RULER + GAP + lane(action) * (LANE + GAP);
            const selected = action.id === p.selectedId;
            const active = action.id === p.activeId;
            if (action.kind.type === "move" || action.kind.type === "drag") {
                ctx.fillStyle = selected || active ? "#1b6c80" : "#85adb6";
                const left = Math.max(0, x0);
                ctx.fillRect(left, y + 12, Math.max(2, Math.min(width, x1) - left), selected || active ? 5 : 3);
                if (action.kind.type === "drag") {
                    ctx.strokeStyle = "#52676c";
                    ctx.strokeRect(left, y + 7, Math.max(3, Math.min(width, x1) - left), 14);
                }
            } else if (action.kind.type === "click") {
                ctx.fillStyle = selected || active ? "#a7393c" : "#c65b5e";
                const center = Math.max(3, x0);
                ctx.beginPath();
                ctx.arc(center, y + 14, selected ? 5 : 3.5, 0, Math.PI * 2);
                ctx.fill();
                if (action.kind.count === 2) {
                    ctx.beginPath();
                    ctx.arc(center + 6, y + 14, 3, 0, Math.PI * 2);
                    ctx.strokeStyle = ctx.fillStyle;
                    ctx.stroke();
                }
                if (view.zoom >= 2) {
                    ctx.fillStyle = "#525d62";
                    ctx.fillText(
                        action.kind.button === "right" ? "右" : action.kind.button === "middle" ? "中" : "左",
                        center + 6,
                        y + 10,
                    );
                }
            } else if (action.kind.type === "wheel") {
                ctx.fillStyle = "#b8873c";
                ctx.fillRect(Math.max(1, x0), y + 6, 3, 16);
            } else if (action.kind.type === "wait") {
                ctx.strokeStyle = selected || active ? "#997332" : "#b9a780";
                ctx.setLineDash([3, 2]);
                ctx.strokeRect(Math.max(0, x0), y + 7, Math.max(3, Math.min(width, x1) - Math.max(0, x0)), 14);
                ctx.setLineDash([]);
            } else {
                ctx.fillStyle = selected || active ? "#1b6c80" : "#566269";
                const blockW = Math.max(5, Math.min(width, x1) - Math.max(0, x0));
                ctx.fillRect(Math.max(0, x0), y + 6, blockW, 16);
                if (blockW > 55) {
                    ctx.fillStyle = "white";
                    ctx.save();
                    ctx.beginPath();
                    ctx.rect(Math.max(0, x0), y, blockW, LANE);
                    ctx.clip();
                    ctx.fillText(
                        action.kind.type === "keys" ? action.kind.chord : action.kind.text,
                        Math.max(0, x0) + 4,
                        y + 18,
                    );
                    ctx.restore();
                }
            }
        }
        if (p.recording.durationUs > 0 && endX >= 0 && endX <= width) {
            ctx.strokeStyle = "#bd5357";
            ctx.setLineDash([3, 3]);
            ctx.beginPath();
            ctx.moveTo(endX, 3);
            ctx.lineTo(endX, HEIGHT);
            ctx.stroke();
            ctx.setLineDash([]);
            ctx.fillStyle = "#a8474b";
            ctx.fillText("结束", Math.min(width - 27, endX + 3), 12);
        }
        const head = xOf(p.timeUs);
        if (head >= 0 && head <= width) {
            ctx.strokeStyle = "#df5457";
            ctx.lineWidth = 2;
            ctx.beginPath();
            ctx.moveTo(head, 0);
            ctx.lineTo(head, HEIGHT);
            ctx.stroke();
        }
    }, [p.rows, p.timeUs, p.selectedId, p.activeId, p.recording.durationUs, width, view.zoom, start, span]);
    const hit = (clientX: number, clientY: number) => {
        const rect = canvasRef.current!.getBoundingClientRect();
        const x = clientX - rect.left;
        const y = clientY - rect.top;
        const rowLane = Math.floor((y - RULER - GAP) / (LANE + GAP));
        const time = start + (x / width) * span;
        const tolerance = (span / width) * 7;
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
                        disabled={view.zoom <= 1}
                        onClick={() => zoomAt(0.8, width / 2)}
                    >
                        <Icon name="minus" />
                    </button>
                    <button type="button" className="zoom-fit-label" onClick={() => setView({ zoom: 1, startUs: 0 })}>
                        适合窗口
                    </button>
                    <button
                        type="button"
                        className="icon-button"
                        aria-label="放大时间线"
                        disabled={view.zoom >= 64}
                        onClick={() => zoomAt(1.25, width / 2)}
                    >
                        <Icon name="plus" />
                    </button>
                    <span className="muted zoom-level">{Math.round(view.zoom * 100)}%</span>
                </div>
                <span className="muted timeline-help">滚轮缩放 · Shift＋滚轮平移 · 双击编辑</span>
            </div>
            <div className="timeline-body">
                <div className="lane-labels">
                    <div className="ruler-label">时间</div>
                    <div className="lane-label">
                        <Icon name="mouse" />
                        鼠标
                    </div>
                    <div className="lane-label">
                        <Icon name="keyboard" />
                        键盘
                    </div>
                    <div className="lane-label">等待</div>
                </div>
                <div className="timeline-track" ref={trackRef}>
                    <canvas
                        ref={canvasRef}
                        style={{ height: HEIGHT }}
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
