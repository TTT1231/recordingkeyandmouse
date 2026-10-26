import { useEffect, useRef, useState } from "react";
import { formatShortClock } from "../format";
import type { ActionView } from "../types";
import { Icon } from "./Icon";
interface Props {
    hidden?: boolean;
    rows: ActionView[];
    selectedId: string | null;
    recordingLive: boolean;
    disabled: boolean;
    onSelect: (action: ActionView) => void;
    onEdit: (action: ActionView) => void;
    onInsertWait: (after: string | null) => void;
    onInsertText: (after: string | null) => void;
    onCollapse: () => void;
}
export function ActionsPanel(p: Props) {
    const listRef = useRef<HTMLDivElement>(null);
    const [limit, setLimit] = useState(200);
    const [context, setContext] = useState<{ id: string; x: number; y: number } | null>(null);
    useEffect(() => {
        setLimit(200);
    }, [p.rows]);
    useEffect(() => {
        if (!p.selectedId) return;
        const index = p.rows.findIndex(row => row.id === p.selectedId);
        if (index >= limit) setLimit(index + 50);
        const element = listRef.current?.querySelector(`[data-action-id="${p.selectedId}"]`);
        element?.scrollIntoView({ block: "nearest" });
    }, [p.selectedId, p.rows, limit]);
    useEffect(() => {
        if (!context) return;
        const close = () => setContext(null);
        window.addEventListener("click", close);
        return () => window.removeEventListener("click", close);
    }, [context]);
    const openContext = (row: ActionView, x: number, y: number) => {
        if (p.disabled) return;
        p.onSelect(row);
        setContext({
            id: row.id,
            x: Math.min(x, window.innerWidth - 208),
            y: Math.min(y, window.innerHeight - 130),
        });
    };
    return (
        <aside hidden={p.hidden} className="actions-panel" aria-label="录制内容" data-tour="actions">
            <div className="actions-heading">
                <h2>
                    录制内容 <span className="event-count">{p.rows.length}</span>
                </h2>
                <button
                    type="button"
                    className="icon-button"
                    aria-label="插入等待"
                    title="在选中动作后插入等待"
                    disabled={p.disabled || p.recordingLive || !p.selectedId}
                    onClick={() => p.onInsertWait(p.selectedId)}
                >
                    <Icon name="plus" size={16} />
                </button>
                <button
                    type="button"
                    className="icon-button"
                    aria-label="收起录制内容"
                    title="收起录制内容"
                    onClick={p.onCollapse}
                >
                    <Icon name="x" size={16} />
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
                            className={`event-row${row.id === p.selectedId ? " selected" : ""}`}
                            role="option"
                            aria-selected={row.id === p.selectedId}
                            tabIndex={0}
                            title={`${row.title}\n${row.detail}\n${formatShortClock(row.startUs)} – ${formatShortClock(row.endUs)}\n${row.eventCount} 个底层事件${row.editable ? "\n双击编辑" : "\n此动作保留原始输入，不提供参数编辑"}`}
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
                                openContext(row, e.clientX, e.clientY);
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
                                <span className="event-title">{row.title}</span>
                                <span className="event-detail">{row.detail}</span>
                            </span>
                            <time className="event-time">{formatShortClock(row.startUs)}</time>
                            <button
                                type="button"
                                className="event-more icon-button"
                                aria-label={`${row.title}的更多操作`}
                                onClick={e => {
                                    e.stopPropagation();
                                    openContext(row, e.clientX, e.clientY);
                                }}
                            >
                                <Icon name="more" size={15} />
                            </button>
                        </div>
                    ))}
                {p.recordingLive && <p className="muted list-note">录制中，结束后整理为动作。</p>}
                {!p.recordingLive && p.rows.length >= limit && (
                    <button type="button" className="text-button" onClick={() => setLimit(current => current + 200)}>
                        继续显示后续动作（共 {p.rows.length} 个）
                    </button>
                )}
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
                        编辑
                    </button>
                    <button type="button" onClick={() => p.onInsertWait(context.id)}>
                        在后面插入等待
                    </button>
                    <button type="button" onClick={() => p.onInsertText(context.id)}>
                        在后面插入文本
                    </button>
                </div>
            )}
        </aside>
    );
}
