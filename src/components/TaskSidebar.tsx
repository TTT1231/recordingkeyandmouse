import { useEffect, useState } from "react";

import { formatSeconds } from "../format";
import type { DocumentView, SessionSnapshot, TaskSummary } from "../types";
import { Icon } from "./Icon";
interface Props {
    snapshot: SessionSnapshot;
    document: DocumentView;
    tasks: TaskSummary[];
    disabled: boolean;
    collapsed: boolean;
    onCreate: () => void;
    onSelect: (id: string) => void;
    onRename: () => void;
    onDelete: () => void;
}
export function TaskSidebar(p: Props) {
    const [context, setContext] = useState<{ id: string; x: number; y: number } | null>(null);
    useEffect(() => {
        if (!context) return;
        const close = () => setContext(null);
        const key = (event: KeyboardEvent) => {
            if (event.key === "Escape") setContext(null);
        };
        window.addEventListener("click", close);
        window.addEventListener("keydown", key);
        return () => {
            window.removeEventListener("click", close);
            window.removeEventListener("keydown", key);
        };
    }, [context]);
    const contextSelected = context !== null && context.id === p.snapshot.taskId;
    return (
        <aside
            id="task-sidebar"
            className="task-sidebar"
            aria-label="我的任务"
            aria-hidden={p.collapsed || undefined}
            data-tour="tasks"
            inert={p.collapsed || undefined}
            onContextMenu={event => {
                if (!(event.target instanceof Element && event.target.closest(".task-row"))) setContext(null);
            }}
        >
            <button
                type="button"
                className="new-task-button"
                disabled={p.disabled || p.snapshot.isDemo}
                onClick={p.onCreate}
            >
                <Icon name="plus" size={16} />
                新建任务
            </button>
            <div className="task-section-label">我的任务</div>
            <nav className="task-list" aria-label="选择任务">
                {p.tasks.map(task => {
                    const selected = task.id === p.snapshot.taskId;
                    return (
                        <div
                            key={task.id}
                            className={`task-row${selected ? " selected" : ""}`}
                            onContextMenu={event => {
                                event.preventDefault();
                                if (p.disabled) return;
                                if (!selected) p.onSelect(task.id);
                                setContext({
                                    id: task.id,
                                    x: Math.min(event.clientX, window.innerWidth - 216),
                                    y: Math.min(event.clientY, window.innerHeight - 104),
                                });
                            }}
                        >
                            <button
                                type="button"
                                className="task-select"
                                aria-current={selected ? "page" : undefined}
                                disabled={p.disabled}
                                onClick={() => {
                                    if (!selected) p.onSelect(task.id);
                                }}
                                onDoubleClick={() => {
                                    if (selected && !p.snapshot.isDemo) p.onRename();
                                }}
                            >
                                <span className="task-icon">
                                    <Icon name={task.actionCount ? "repeat" : "folder"} size={17} />
                                </span>
                                <span className="task-name">{task.name}</span>
                                <span className="task-meta">
                                    {task.actionCount ? formatSeconds(task.durationUs) : "尚未录制"}
                                </span>
                            </button>
                        </div>
                    );
                })}
            </nav>
            {context && (
                <div
                    className="context-menu"
                    role="menu"
                    aria-label="任务操作"
                    style={{ left: context.x, top: context.y }}
                >
                    <button
                        type="button"
                        role="menuitem"
                        disabled={!contextSelected || p.disabled || p.snapshot.isDemo}
                        onClick={() => {
                            setContext(null);
                            p.onRename();
                        }}
                    >
                        重命名
                    </button>
                    <button
                        type="button"
                        role="menuitem"
                        className="danger-text"
                        disabled={!contextSelected || p.disabled || p.snapshot.isDemo}
                        onClick={() => {
                            setContext(null);
                            p.onDelete();
                        }}
                    >
                        删除任务…
                    </button>
                </div>
            )}
        </aside>
    );
}
