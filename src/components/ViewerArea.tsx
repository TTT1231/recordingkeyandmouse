import { useEffect, useMemo, useRef, useState } from "react";
import { desktopBounds } from "../actions";
import { formatClock, vkName } from "../format";
import { createCursor, seek } from "../preview";
import type { ActionView, DocumentView } from "../types";
import { Icon } from "./Icon";
interface Props {
    recording: DocumentView;
    rows: ActionView[];
    timeUs: number;
    playing: boolean;
    speed: number;
    disabled?: boolean;
    picking?: boolean;
    currentAction: ActionView | null;
    onTimeChange: (timeUs: number) => void;
    onTogglePlay: () => void;
    onEnd: () => void;
    onSpeedChange: (speed: number) => void;
    onSelectAction: (action: ActionView) => void;
    onPick?: (x: number, y: number) => void;
}
export function ViewerArea(p: Props) {
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const frameRef = useRef<HTMLDivElement>(null);
    const cursorRef = useRef(createCursor());
    const cursorKey = useRef("");
    const timeRef = useRef(p.timeUs);
    timeRef.current = p.timeUs;
    const [size, setSize] = useState({ width: 640, height: 360 });
    const [image, setImage] = useState<HTMLImageElement | null>(null);
    const bounds = useMemo(() => desktopBounds(p.recording.geometry.monitors), [p.recording.geometry]);
    const mapping = useMemo(() => {
        const scale = Math.min(size.width / bounds.width, size.height / bounds.height);
        return {
            scale,
            offX: (size.width - bounds.width * scale) / 2,
            offY: (size.height - bounds.height * scale) / 2,
        };
    }, [bounds, size]);
    const cursor = useMemo(() => {
        const key = `${p.recording.taskId}:${p.recording.revision}`;
        if (cursorKey.current !== key) {
            cursorRef.current = createCursor();
            cursorKey.current = key;
        }
        seek(cursorRef.current, p.recording.events, p.timeUs);
        return cursorRef.current;
    }, [p.recording, p.timeUs]);
    useEffect(() => {
        const frame = frameRef.current;
        if (!frame) return;
        const observer = new ResizeObserver(() =>
            setSize({ width: Math.max(1, frame.clientWidth), height: Math.max(1, frame.clientHeight) }),
        );
        observer.observe(frame);
        return () => observer.disconnect();
    }, []);
    useEffect(() => {
        let active = true;
        setImage(null);
        if (p.recording.referenceImage) {
            const next = new Image();
            next.onload = () => {
                if (active) setImage(next);
            };
            next.src = p.recording.referenceImage.dataUrl;
        }
        return () => {
            active = false;
        };
    }, [p.recording.referenceImage]);
    useEffect(() => {
        if (!p.playing || p.disabled) return;
        let raf = 0;
        let last = performance.now();
        const tick = (now: number) => {
            const next = Math.min(p.recording.durationUs, timeRef.current + (now - last) * 1000 * p.speed);
            last = now;
            p.onTimeChange(next);
            if (next >= p.recording.durationUs) {
                p.onEnd();
                return;
            }
            raf = requestAnimationFrame(tick);
        };
        raf = requestAnimationFrame(tick);
        return () => cancelAnimationFrame(raf);
    }, [p.playing, p.disabled, p.speed, p.recording.durationUs, p.onTimeChange, p.onEnd]);
    useEffect(() => {
        const canvas = canvasRef.current;
        const ctx = canvas?.getContext("2d");
        if (!canvas || !ctx) return;
        const dpr = window.devicePixelRatio || 1;
        canvas.width = Math.max(1, Math.round(size.width * dpr));
        canvas.height = Math.max(1, Math.round(size.height * dpr));
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        ctx.clearRect(0, 0, size.width, size.height);
        ctx.fillStyle = "#f2f3f4";
        ctx.fillRect(0, 0, size.width, size.height);
        const toCanvas = (x: number, y: number) => [
            mapping.offX + (x - bounds.x) * mapping.scale,
            mapping.offY + (y - bounds.y) * mapping.scale,
        ];
        if (image && p.recording.referenceImage) {
            const area = p.recording.referenceImage.mapping;
            const ratio = Math.min(area.width / image.width, area.height / image.height);
            const w = image.width * ratio;
            const h = image.height * ratio;
            const [x, y] = toCanvas(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2);
            ctx.drawImage(image, x, y, w * mapping.scale, h * mapping.scale);
        }
        ctx.strokeStyle = "#d8dde0";
        ctx.lineWidth = 1;
        for (const monitor of p.recording.geometry.monitors) {
            const [x, y] = toCanvas(monitor.x, monitor.y);
            ctx.strokeRect(x + 0.5, y + 0.5, monitor.width * mapping.scale, monitor.height * mapping.scale);
        }
        if (cursor.trail.length > 1) {
            ctx.strokeStyle = "rgba(27,108,128,.38)";
            ctx.lineWidth = 1.4;
            ctx.beginPath();
            const first = toCanvas(cursor.trail[0].x, cursor.trail[0].y);
            ctx.moveTo(first[0], first[1]);
            const stride = Math.max(1, Math.ceil(cursor.trail.length / 2400));
            for (let i = stride; i < cursor.trail.length; i += stride) {
                const [x, y] = toCanvas(cursor.trail[i].x, cursor.trail[i].y);
                ctx.lineTo(x, y);
            }
            const tail = cursor.trail[cursor.trail.length - 1];
            const [x, y] = toCanvas(tail.x, tail.y);
            ctx.lineTo(x, y);
            ctx.stroke();
        }
        if (cursor.lastClick && p.timeUs - cursor.lastClick.atUs < 450_000) {
            const flash = cursor.lastClick;
            const [x, y] = toCanvas(flash.x, flash.y);
            const fade = Math.max(0, 1 - (p.timeUs - flash.atUs) / 450_000);
            ctx.fillStyle =
                flash.button === "right" ? `rgba(195,128,45,${fade * 0.5})` : `rgba(223,84,87,${fade * 0.5})`;
            ctx.beginPath();
            ctx.arc(x, y, 10 + 8 * (1 - fade), 0, Math.PI * 2);
            ctx.fill();
        }
        if (cursor.lastWheel && p.timeUs - cursor.lastWheel.atUs < 500_000) {
            const wheel = cursor.lastWheel;
            const [x, y] = toCanvas(wheel.x, wheel.y);
            ctx.fillStyle = "#946315";
            ctx.font = "bold 24px sans-serif";
            ctx.fillText(
                wheel.axis === "vertical" ? (wheel.delta > 0 ? "↑" : "↓") : wheel.delta > 0 ? "→" : "←",
                x + 12,
                y,
            );
        }
        if (cursor.hasPos) {
            const [x, y] = toCanvas(cursor.x, cursor.y);
            ctx.save();
            ctx.translate(x, y);
            ctx.fillStyle = cursor.buttonsDown.size ? "#c65356" : "#202729";
            ctx.beginPath();
            ctx.moveTo(0, 0);
            ctx.lineTo(0, 16);
            ctx.lineTo(4, 12);
            ctx.lineTo(8, 19);
            ctx.lineTo(11, 17);
            ctx.lineTo(8, 11);
            ctx.lineTo(13, 10);
            ctx.closePath();
            ctx.fill();
            ctx.restore();
        }
    }, [cursor, p.timeUs, p.recording, bounds, mapping, image, size]);
    const jump = (direction: -1 | 1) => {
        const action =
            direction < 0
                ? [...p.rows].reverse().find(row => row.startUs < p.timeUs - 1000)
                : p.rows.find(row => row.startUs > p.timeUs + 1000);
        if (action) p.onSelectAction(action);
    };
    const keys = [...cursor.keysDown].map(vkName);
    return (
        <section className="viewer-area" aria-label="键鼠回放预览">
            <div className={`preview-viewport${p.picking ? " picking" : ""}`} ref={frameRef}>
                <div className="screen-canvas">
                    <canvas
                        ref={canvasRef}
                        aria-label="键鼠轨迹预览"
                        onClick={e => {
                            if (!p.picking || !p.onPick) return;
                            const rect = e.currentTarget.getBoundingClientRect();
                            const x = Math.round(bounds.x + (e.clientX - rect.left - mapping.offX) / mapping.scale);
                            const y = Math.round(bounds.y + (e.clientY - rect.top - mapping.offY) / mapping.scale);
                            if (
                                p.recording.geometry.monitors.some(
                                    m => x >= m.x && y >= m.y && x < m.x + m.width && y < m.y + m.height,
                                )
                            )
                                p.onPick(x, y);
                        }}
                    />
                    {keys.length > 0 && (
                        <div className="key-feedback">
                            {keys.map(key => (
                                <kbd key={key}>{key}</kbd>
                            ))}
                        </div>
                    )}
                    {cursor.textFeedback && p.timeUs - cursor.textAtUs < 700_000 && (
                        <div className="text-feedback">{cursor.textFeedback}</div>
                    )}
                </div>
                <div className="preview-caption">
                    {p.picking
                        ? "点击桌面范围内的位置，设置动作终点"
                        : p.recording.referenceImage
                          ? `参考图：${p.recording.referenceImage.name} · 手动导入，仅供对照`
                          : "无参考图 · 仅显示录制的键鼠轨迹"}
                </div>
            </div>
            <div className="current-action" aria-live="polite">
                {p.currentAction ? (
                    <>
                        <strong>{p.currentAction.title}</strong>
                        <span>{p.currentAction.detail}</span>
                    </>
                ) : (
                    <span>
                        {p.timeUs > 0 && p.timeUs < p.recording.durationUs
                            ? "原始停顿"
                            : p.rows.length
                              ? "选择动作或播放预览"
                              : "开始录制，或插入文本创建任务内容"}
                    </span>
                )}
            </div>
            <div className="player">
                <div className="player-time">
                    <strong>{formatClock(p.timeUs)}</strong>
                    <span> / {formatClock(p.recording.durationUs)}</span>
                </div>
                <div className="player-buttons">
                    <button
                        type="button"
                        className="icon-button"
                        disabled={p.disabled || !p.rows.length}
                        aria-label="上一个动作"
                        title="上一个动作"
                        onClick={() => jump(-1)}
                    >
                        <Icon name="previous" />
                    </button>
                    <button
                        type="button"
                        className="play-button"
                        disabled={p.disabled || p.recording.durationUs <= 0}
                        aria-label={p.playing ? "暂停预览" : "播放预览"}
                        title={p.playing ? "暂停预览" : "播放预览（不操作系统）"}
                        onClick={p.onTogglePlay}
                    >
                        <Icon name={p.playing ? "pause" : "play"} size={18} />
                    </button>
                    <button
                        type="button"
                        className="icon-button"
                        disabled={p.disabled || !p.rows.length}
                        aria-label="下一个动作"
                        title="下一个动作"
                        onClick={() => jump(1)}
                    >
                        <Icon name="next" />
                    </button>
                </div>
                <select
                    className="speed-select"
                    aria-label="预览速度"
                    value={p.speed}
                    disabled={p.disabled}
                    onChange={e => p.onSpeedChange(Number(e.currentTarget.value))}
                >
                    <option value={0.5}>0.5 倍</option>
                    <option value={1}>原速</option>
                    <option value={2}>2 倍</option>
                </select>
            </div>
        </section>
    );
}
