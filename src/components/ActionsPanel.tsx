import { useEffect, useRef, useState } from "react";
import { actionLabel } from "../actionLabel";
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
    onDeleteAction: (id: string) => void;
    onCollapse: () => void;
}
export function ActionsPanel(p: Props) {
    const listRef = useRef<HTMLDivElement>(null);
    const [context, setContext] = useState<{ id: string; x: number; y: number } | null>(null);
    useEffect(() => {
        if (!p.selectedId) return;
        const list = listRef.current;
        const element = list?.querySelector(`[data-action-id="${p.selectedId}"]`);
        if (!list || !element) return;
        const listBounds = list.getBoundingClientRect();
        const rowBounds = element.getBoundingClientRect();
        if (rowBounds.top < listBounds.top) list.scrollTop += rowBounds.top - listBounds.top;
        else if (rowBounds.bottom > listBounds.bottom) list.scrollTop += rowBounds.bottom - listBounds.bottom;
    }, [p.selectedId, p.rows]);
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
            y: Math.min(y, window.innerHeight - 170),
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
                    aria-label="收起录制内容"
                    title="收起录制内容"
                    onClick={p.onCollapse}
                >
                    <Icon name="x" size={16} />
                </button>
            </div>
            <div className="event-list" ref={listRef} role="listbox" aria-label="录下的动作">
                {!p.recordingLive &&
                    p.rows.map(row => {
                        const label = actionLabel(row);
                        return (
                            <div
                                key={row.id}
                                data-action-id={row.id}
                                data-tone={label.tone}
                                className={`event-row${row.id === p.selectedId ? " selected" : ""}`}
                                role="option"
                                aria-selected={row.id === p.selectedId}
                                tabIndex={0}
                                onClick={() => p.onSelect(row)}
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
                                    <Icon name={label.icon} size={18} />
                                </span>
                                <span className="event-copy">
                                    <span className="event-title">{label.title}</span>
                                    {label.value && <span className="event-value">{label.value}</span>}
                                </span>
                                <time className="event-time">{formatShortClock(row.startUs)}</time>
                            </div>
                        );
                    })}
                {p.recordingLive && <p className="muted list-note">录制中，结束后整理为动作。</p>}
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
                        插入等待
                    </button>
                    <button type="button" onClick={() => p.onInsertText(context.id)}>
                        插入文本
                    </button>
                    <div className="menu-separator" />
                    <button type="button" className="danger-text" onClick={() => p.onDeleteAction(context.id)}>
                        删除
                    </button>
                </div>
            )}
        </aside>
    );
}
