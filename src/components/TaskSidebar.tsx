import { formatSeconds } from "../format";
import type { DocumentView, SessionSnapshot, TaskSummary } from "../types";
import { Icon } from "./Icon";
interface Props {
    snapshot: SessionSnapshot;
    document: DocumentView;
    tasks: TaskSummary[];
    disabled: boolean;
    onCreate: () => void;
    onSelect: (id: string) => void;
    onRename: () => void;
    onDelete: () => void;
}
export function TaskSidebar(p: Props) {
    return (
        <aside className="task-sidebar" aria-label="我的任务" data-tour="tasks">
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
                        <div key={task.id} className={`task-row${selected ? " selected" : ""}`}>
                            <button
                                type="button"
                                className="task-select"
                                aria-current={selected ? "page" : undefined}
                                disabled={p.disabled}
                                onClick={() => p.onSelect(task.id)}
                                onDoubleClick={() => {
                                    if (selected && !p.snapshot.isDemo) p.onRename();
                                }}
                            >
                                <span className="task-icon">
                                    <Icon name={task.actionCount ? "repeat" : "folder"} size={16} />
                                </span>
                                <span className="task-copy">
                                    <span className="task-name">{task.name}</span>
                                    <span className="task-meta">
                                        {task.actionCount ? formatSeconds(task.durationUs) : "尚未录制"}
                                    </span>
                                </span>
                            </button>
                            {selected && (
                                <details className="app-menu task-more">
                                    <summary aria-label={`${task.name}的更多操作`}>
                                        <Icon name="more" size={15} />
                                    </summary>
                                    <div className="menu-popup">
                                        <button
                                            type="button"
                                            disabled={p.disabled || p.snapshot.isDemo}
                                            onClick={p.onRename}
                                        >
                                            重命名
                                        </button>
                                        <button
                                            type="button"
                                            className="danger-text"
                                            disabled={p.disabled || p.snapshot.isDemo}
                                            onClick={p.onDelete}
                                        >
                                            删除任务…
                                        </button>
                                    </div>
                                </details>
                            )}
                        </div>
                    );
                })}
            </nav>
            <div className="task-sidebar-footer">本地任务</div>
        </aside>
    );
}
