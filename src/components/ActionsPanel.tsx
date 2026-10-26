import { useEffect, useRef, useState } from "react";
import { formatClock } from "../format";
import type { ActionView } from "../types";
import { Icon } from "./Icon";
interface Props {
    hidden?: boolean;
    rows: ActionView[];
    selectedId: string | null;
    activeId: string | null;
    playing: boolean;
    recordingLive: boolean;
    disabled: boolean;
    onSelect: (action: ActionView) => void;
    onEdit: (action: ActionView) => void;
    onInsertWait: (after: string | null) => void;
    onInsertText: (after: string | null) => void;
}
export function ActionsPanel(p: Props) {
    const listRef = useRef<HTMLDivElement>(null);
    const [limit, setLimit] = useState(200);
    const [context, setContext] = useState<{ id: string; x: number; y: number } | null>(null);
    useEffect(() => {
        setLimit(200);
    }, [p.rows]);
    useEffect(() => {
        const id = p.playing ? p.activeId : p.selectedId;
        if (!id) return;
        const index = p.rows.findIndex(row => row.id === id);
        if (index >= limit) setLimit(index + 50);
        const element = listRef.current?.querySelector(`[data-action-id="${id}"]`);
        element?.scrollIntoView({ block: "nearest" });
    }, [p.selectedId, p.activeId, p.playing, p.rows, limit]);
    useEffect(() => {
        if (!context) return;
        const close = () => setContext(null);
        window.addEventListener("click", close);
        return () => window.removeEventListener("click", close);
    }, [context]);
    return (
        <aside hidden={p.hidden} className="actions-panel" aria-label="录制内容" data-tour="actions">
            <div className="actions-heading">
                <h2>
                    录制内容 <span className="event-count">{p.rows.length} 个动作</span>
                </h2>
                <button
                    type="button"
                    className="text-button"
                    disabled={p.disabled || p.recordingLive}
                    onClick={() => p.onInsertText(p.selectedId)}
                >
                    插入文本
                </button>
            </div>
            <div
                className="event-list"
                ref={listRef}
                role="listbox"
                aria-label="录下的动作"
                onScroll={e => {
                    if (e.currentTarget.scrollTop + e.currentTarget.clientHeight >= e.currentTarget.scrollHeight - 120)
                        setLimit(current => Math.min(p.rows.length, current + 150));
                }}
            >
                {!p.recordingLive &&
                    p.rows.slice(0, limit).map(row => (
                        <div
                            key={row.id}
                            data-action-id={row.id}
                            className={`event-row${row.id === p.selectedId ? " selected" : ""}${row.id === p.activeId ? " current" : ""}`}
                            role="option"
                            aria-selected={row.id === p.selectedId}
                            tabIndex={0}
                            title={`${row.title}\n${row.detail}\n${formatClock(row.startUs)} – ${formatClock(row.endUs)}\n${row.eventCount} 个底层事件${row.editable ? "\n双击编辑" : "\n此动作保留原始输入，不提供参数编辑"}`}
                            onClick={() => p.onSelect(row)}
                            onDoubleClick={() => !p.disabled && p.onEdit(row)}
                            onKeyDown={e => {
                                if (e.key === "Enter") {
                                    e.preventDefault();
                                    p.onSelect(row);
                                }
                                if (e.key === "F2" && !p.disabled) {
                                    e.preventDefault();
                                    p.onEdit(row);
                                }
                            }}
                            onContextMenu={e => {
                                e.preventDefault();
                                if (!p.disabled) {
                                    p.onSelect(row);
                                    setContext({
                                        id: row.id,
                                        x: Math.min(e.clientX, window.innerWidth - 180),
                                        y: Math.min(e.clientY, window.innerHeight - 110),
                                    });
                                }
                            }}
                        >
                            <span className="event-icon">
                                <Icon
                                    name={
                                        row.kind.type === "move" || row.kind.type === "drag"
                                            ? "cursor"
                                            : row.kind.type === "keys" || row.kind.type === "text"
                                              ? "keyboard"
                                              : row.kind.type === "wait"
                                                ? "pause"
                                                : "mouse"
                                    }
                                />
                            </span>
                            <span className="event-copy">
                                <span className="event-title">
                                    {row.title}
                                    {p.playing && row.id === p.activeId && <span className="current-label">当前</span>}
                                </span>
                                <span className="event-detail">{row.detail}</span>
                            </span>
                            <time className="event-time">{formatClock(row.startUs)}</time>
                        </div>
                    ))}
                {p.recordingLive ? (
                    <p className="muted list-note">录制中，结束后整理为动作。原任务保持不变。</p>
                ) : !p.rows.length ? (
                    <p className="muted list-note">录制一次操作，或插入文本开始。</p>
                ) : (
                    limit < p.rows.length && (
                        <button
                            type="button"
                            className="text-button"
                            onClick={() => setLimit(current => current + 200)}
                        >
                            继续显示后续动作（共 {p.rows.length} 个）
                        </button>
                    )
                )}
            </div>
            <div className="actions-footer">
                <span className="muted">单击定位 · 双击编辑 · 右键插入</span>
                <button
                    type="button"
                    className="text-button"
                    disabled={p.disabled || p.recordingLive || !p.rows.length}
                    onClick={() => p.onInsertWait(p.selectedId)}
                >
                    插入等待
                </button>
            </div>
            {context && (
                <div className="context-menu" style={{ left: context.x, top: context.y }}>
                    <button
                        type="button"
                        onClick={() => {
                            const row = p.rows.find(a => a.id === context.id);
                            if (row) p.onEdit(row);
                        }}
                    >
                        编辑动作
                    </button>
                    <button type="button" onClick={() => p.onInsertWait(context.id)}>
                        在其后插入等待
                    </button>
                    <button type="button" onClick={() => p.onInsertText(context.id)}>
                        在其后插入文本
                    </button>
                </div>
            )}
        </aside>
    );
}
