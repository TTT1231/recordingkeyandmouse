import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
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
    empty?: boolean;
    isDemo?: boolean;
    onTimeChange: (timeUs: number) => void;
    onTogglePlay: () => void;
    onEnd: () => void;
    onSpeedChange: (speed: number) => void;
    onSelectAction: (action: ActionView) => void;
    onPick?: (x: number, y: number) => void;
    onAddReference?: () => void;
    timelineOpen?: boolean;
    onCloseTimeline?: () => void;
}
function practiceHit(x: number, y: number): number | "save" | "submit" | null {
    if (x >= 320 && x <= 910) {
        if (y >= 248 && y <= 306) return 0;
        if (y >= 364 && y <= 422) return 1;
        if (y >= 480 && y <= 538) return 2;
    }
    if (x >= 848 && x <= 1028 && y >= 619 && y <= 681) return "submit";
    if (x >= 657 && x <= 833 && y >= 619 && y <= 681) return "save";
    return null;
}
function demoFrame(rows: ActionView[], timeUs: number) {
    let fields = ["", "", ""];
    let focus = 0;
    let saved = false;
    let submitted = false;
    for (const action of rows) {
        if (action.startUs > timeUs) break;
        if (action.kind.type === "click") {
            const hit = practiceHit(action.kind.x, action.kind.y);
            if (typeof hit === "number") focus = hit;
            if (hit === "save" && timeUs >= action.endUs) saved = true;
            if (hit === "submit" && timeUs >= action.endUs && fields[0]) submitted = true;
        } else if (action.kind.type === "text") {
            const progress =
                action.endUs > action.startUs
                    ? Math.min(1, Math.max(0, (timeUs - action.startUs) / (action.endUs - action.startUs)))
                    : 1;
            const chars = Array.from(action.kind.text);
            fields[focus] += chars.slice(0, Math.max(1, Math.ceil(chars.length * progress))).join("");
        } else if (action.kind.type === "keys") {
            const chord = action.kind.chord;
            if (chord === "Tab") focus = (focus + 1) % 3;
            else if (/^[A-Za-z]$/.test(chord)) fields[focus] += chord.toLowerCase();
            else if (chord.includes("Ctrl") && chord.endsWith("S")) saved = true;
            else if (chord === "Enter" && timeUs >= action.endUs && fields[0]) submitted = true;
        }
    }
    return { fields, focus, saved, submitted };
}
function DemoScreen({
    rows,
    timeUs,
    playing,
    scale,
}: {
    rows: ActionView[];
    timeUs: number;
    playing: boolean;
    scale: number;
}) {
    const frame = useMemo(() => demoFrame(rows, timeUs), [rows, timeUs]);
    const [values, setValues] = useState(frame.fields);
    const [focus, setFocus] = useState(frame.focus);
    const [saved, setSaved] = useState(frame.saved);
    const [submitted, setSubmitted] = useState(frame.submitted);
    useEffect(() => {
        setValues(frame.fields);
        setFocus(frame.focus);
        setSaved(frame.saved);
        setSubmitted(frame.submitted);
    }, [frame]);
    const update = (index: number, value: string) => {
        setValues(current => current.map((item, i) => (i === index ? value : item)));
    };
    return (
        <div className="demo-screen" aria-hidden="true" style={{ "--scene-scale": scale } as CSSProperties}>
            <div className="practice-window">
                <div className="practice-titlebar">
                    <Icon name="folder" size={22} />
                    <span>练习用表单</span>
                    <span className="practice-window-control">—　□　×</span>
                </div>
                <div className="practice-content">
                    <div className="practice-heading">
                        <h3>新建联系人</h3>
                        <span>{submitted ? "已提交" : saved ? "已保存" : "草稿"}</span>
                    </div>
                    <label className="practice-label name-label" htmlFor="practice-name">
                        姓名
                    </label>
                    <input
                        className={`practice-input name-input${focus === 0 && !submitted ? " replay-focus" : ""}`}
                        id="practice-name"
                        placeholder="请输入姓名"
                        autoComplete="off"
                        value={values[0]}
                        readOnly={playing}
                        onChange={e => update(0, e.target.value)}
                    />
                    <label className="practice-label phone-label" htmlFor="practice-phone">
                        联系电话
                    </label>
                    <input
                        className={`practice-input phone-input${focus === 1 && !submitted ? " replay-focus" : ""}`}
                        id="practice-phone"
                        placeholder="请输入联系电话"
                        autoComplete="off"
                        value={values[1]}
                        readOnly={playing}
                        onChange={e => update(1, e.target.value)}
                    />
                    <label className="practice-label company-label" htmlFor="practice-company">
                        公司
                    </label>
                    <input
                        className={`practice-input company-input${focus === 2 && !submitted ? " replay-focus" : ""}`}
                        id="practice-company"
                        placeholder="选填"
                        autoComplete="off"
                        value={values[2]}
                        readOnly={playing}
                        onChange={e => update(2, e.target.value)}
                    />
                    <div className="practice-footer">
                        <span>{saved ? "已保存" : "未保存"}</span>
                        <button type="button" onClick={() => setSaved(true)}>
                            保存草稿
                        </button>
                        <button
                            type="button"
                            className="practice-submit"
                            onClick={() => {
                                if (values[0]) setSubmitted(true);
                            }}
                        >
                            提交
                        </button>
                    </div>
                    {submitted && (
                        <div className="practice-success">
                            <Icon name="check" size={56} />
                            <strong>资料已提交</strong>
                        </div>
                    )}
                </div>
            </div>
        </div>
    );
}
export function ViewerArea(p: Props) {
    const viewportRef = useRef<HTMLDivElement>(null);
    const canvasRef = useRef<HTMLCanvasElement>(null);
    const cursorRef = useRef(createCursor());
    const cursorKey = useRef("");
    const timeRef = useRef(p.timeUs);
    timeRef.current = p.timeUs;
    const [size, setSize] = useState({ width: 640, height: 400 });
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
        const viewport = viewportRef.current;
        if (!viewport) return;
        const fit = () => {
            const style = getComputedStyle(viewport);
            const width = Math.max(
                1,
                viewport.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight),
            );
            const height = Math.max(
                1,
                viewport.clientHeight - parseFloat(style.paddingTop) - parseFloat(style.paddingBottom),
            );
            const scale = Math.max(0.01, Math.min(width / bounds.width, height / bounds.height));
            setSize({ width: Math.max(1, bounds.width * scale), height: Math.max(1, bounds.height * scale) });
        };
        const observer = new ResizeObserver(fit);
        observer.observe(viewport);
        fit();
        return () => observer.disconnect();
    }, [bounds]);
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
        ctx.fillStyle = "#f1f2f3";
        ctx.fillRect(0, 0, size.width, size.height);
        if (image && p.recording.referenceImage) {
            const area = p.recording.referenceImage.mapping;
            const ratio = Math.min(area.width / image.width, area.height / image.height);
            const w = image.width * ratio;
            const h = image.height * ratio;
            const x = mapping.offX + (area.x + (area.width - w) / 2 - bounds.x) * mapping.scale;
            const y = mapping.offY + (area.y + (area.height - h) / 2 - bounds.y) * mapping.scale;
            ctx.drawImage(image, x, y, w * mapping.scale, h * mapping.scale);
        }
    }, [p.recording, bounds, mapping, image, size]);
    const jump = (direction: -1 | 1) => {
        const action =
            direction < 0
                ? [...p.rows].reverse().find(row => row.startUs < p.timeUs - 1000)
                : p.rows.find(row => row.startUs > p.timeUs + 1000);
        if (action) p.onSelectAction(action);
    };
    const keys = [...cursor.keysDown].map(vkName);
    const click =
        cursor.lastClick && p.timeUs - cursor.lastClick.atUs < 450_000
            ? {
                  x: mapping.offX + (cursor.lastClick.x - bounds.x) * mapping.scale,
                  y: mapping.offY + (cursor.lastClick.y - bounds.y) * mapping.scale,
                  age: (p.timeUs - cursor.lastClick.atUs) / 450_000,
              }
            : null;
    const pointer =
        cursor.hasPos && !p.picking
            ? {
                  x: mapping.offX + (cursor.x - bounds.x) * mapping.scale,
                  y: mapping.offY + (cursor.y - bounds.y) * mapping.scale,
              }
            : null;
    const wheel =
        cursor.lastWheel && p.timeUs - cursor.lastWheel.atUs < 500_000
            ? `${cursor.lastWheel.axis === "vertical" ? (cursor.lastWheel.delta > 0 ? "滚轮向上" : "滚轮向下") : cursor.lastWheel.delta > 0 ? "滚轮向右" : "滚轮向左"}`
            : "";
    return (
        <section className="viewer-area" aria-label="键鼠回放预览">
            <div ref={viewportRef} className={`preview-viewport${p.picking ? " picking" : ""}`} data-tour="preview">
                {p.empty ? (
                    <div className="empty-task-message">
                        <span className="empty-task-icon">
                            <Icon name="record-ring" size={30} />
                        </span>
                        <h2>还没有录制</h2>
                        <p>点击右上角的“开始录制”。</p>
                    </div>
                ) : (
                    <>
                        <div className="screen-canvas" style={{ width: size.width, height: size.height }}>
                            <canvas
                                ref={canvasRef}
                                aria-label="回放画布"
                                onClick={e => {
                                    if (!p.picking || !p.onPick) return;
                                    const rect = e.currentTarget.getBoundingClientRect();
                                    const x = Math.round(
                                        bounds.x + (e.clientX - rect.left - mapping.offX) / mapping.scale,
                                    );
                                    const y = Math.round(
                                        bounds.y + (e.clientY - rect.top - mapping.offY) / mapping.scale,
                                    );
                                    if (
                                        p.recording.geometry.monitors.some(
                                            m => x >= m.x && y >= m.y && x < m.x + m.width && y < m.y + m.height,
                                        )
                                    )
                                        p.onPick(x, y);
                                }}
                            />
                            {p.isDemo && (
                                <DemoScreen
                                    rows={p.rows}
                                    timeUs={p.timeUs}
                                    playing={p.playing}
                                    scale={size.width / 1280}
                                />
                            )}
                            {click && (
                                <span
                                    className="click-feedback"
                                    style={{
                                        left: click.x,
                                        top: click.y,
                                        opacity: Math.max(0, 1 - click.age),
                                    }}
                                />
                            )}
                            {pointer && (
                                <span
                                    className={`replay-pointer${cursor.buttonsDown.size ? " pressed" : ""}`}
                                    style={{ left: pointer.x, top: pointer.y }}
                                >
                                    <svg viewBox="0 0 28 34" aria-hidden="true">
                                        <path d="M3 2v24l6-5 5 11 5-2-5-10 9-1L3 2Z" />
                                    </svg>
                                </span>
                            )}
                            {keys.length > 0 && (
                                <div className="key-feedback">
                                    {keys.map(key => (
                                        <kbd key={key}>{key}</kbd>
                                    ))}
                                </div>
                            )}
                            {wheel && <div className="key-feedback">{wheel}</div>}
                            {cursor.textFeedback && p.timeUs - cursor.textAtUs < 700_000 && (
                                <div className="text-feedback">{cursor.textFeedback}</div>
                            )}
                            {!p.isDemo && !p.disabled && !p.recording.referenceImage && !p.playing && (
                                <div className="background-prompt">
                                    <button type="button" className="background-add" onClick={p.onAddReference}>
                                        <Icon name="image" size={16} />
                                        添加预览背景
                                    </button>
                                </div>
                            )}
                        </div>
                        {!p.isDemo && p.recording.referenceImage && (
                            <div className="viewer-tools">
                                <button
                                    type="button"
                                    className="icon-button"
                                    aria-label={`更换预览背景（当前：${p.recording.referenceImage.name}，手动导入）`}
                                    title={`预览背景：${p.recording.referenceImage.name}（手动导入）`}
                                    onClick={p.onAddReference}
                                >
                                    <Icon name="image" size={16} />
                                </button>
                            </div>
                        )}
                    </>
                )}
            </div>
            {!p.empty && (
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
                    <div id="timeline-zoom-slot" />
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
                    {p.timelineOpen && (
                        <button
                            type="button"
                            className="icon-button timeline-close"
                            aria-label="关闭时间线"
                            title="关闭时间线"
                            onClick={p.onCloseTimeline}
                        >
                            <Icon name="x" size={16} />
                        </button>
                    )}
                </div>
            )}
        </section>
    );
}
