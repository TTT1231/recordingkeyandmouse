import { formatDurationUs } from "../format";
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
                <Icon name="plus" />
                新建任务
            </button>
            <div className="task-section-label">
                我的任务 <span>{p.tasks.length}</span>
            </div>
            <nav className="task-list" aria-label="选择任务">
                {p.tasks.map(task => (
                    <button
                        type="button"
                        key={task.id}
                        className={`task-card${task.id === p.snapshot.taskId ? " selected" : ""}`}
                        aria-current={task.id === p.snapshot.taskId ? "true" : undefined}
                        disabled={p.disabled}
                        onClick={() => p.onSelect(task.id)}
                        onDoubleClick={() => {
                            if (task.id === p.snapshot.taskId) p.onRename();
                        }}
                    >
                        <span className="task-card-name">{task.name}</span>
                        <span className="task-card-meta">
                            {task.actionCount
                                ? `${formatDurationUs(task.durationUs)} · ${task.actionCount} 个动作`
                                : "尚无动作"}
                        </span>
                    </button>
                ))}
            </nav>
            <div className="task-sidebar-footer">
                <span>
                    {p.snapshot.isDemo
                        ? "演示内容与任务隔离"
                        : p.snapshot.hasPendingRecording
                          ? "有录制尚未保存"
                          : "任务保存在此设备"}
                </span>
            </div>
        </aside>
    );
}
