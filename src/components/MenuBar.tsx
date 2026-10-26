import type { SessionSnapshot } from "../types";
interface Props {
    snapshot: SessionSnapshot;
    busy: boolean;
    actionsOpen: boolean;
    timelineOpen: boolean;
    hasSelection: boolean;
    onToggleActions: () => void;
    onToggleTimeline: () => void;
    onCreate: () => void;
    onRename: () => void;
    onSave: () => void;
    onUndo: () => void;
    onRedo: () => void;
    onEdit: () => void;
    onWait: () => void;
    onText: () => void;
    onReference: () => void;
    onDemo: () => void;
    onTour: () => void;
    onShortcuts: () => void;
    onAbout: () => void;
}
export function MenuBar(p: Props) {
    const disabled = p.busy || p.snapshot.mode !== "idle";
    return (
        <nav className="app-menubar" aria-label="应用菜单">
            <details className="app-menu">
                <summary>文件</summary>
                <div className="menu-popup">
                    <button type="button" disabled={disabled || p.snapshot.isDemo} onClick={p.onCreate}>
                        新建任务
                    </button>
                    <button type="button" disabled={disabled || p.snapshot.isDemo} onClick={p.onRename}>
                        重命名任务
                    </button>
                    <button type="button" disabled={disabled || p.snapshot.isDemo} onClick={p.onSave}>
                        保存任务 <kbd>Ctrl+S</kbd>
                    </button>
                    <div className="menu-separator" />
                    <button type="button" disabled={disabled} onClick={p.onReference}>
                        预览参考图…
                    </button>
                </div>
            </details>
            <details className="app-menu">
                <summary>编辑</summary>
                <div className="menu-popup">
                    <button type="button" disabled={disabled || !p.snapshot.canUndo} onClick={p.onUndo}>
                        撤销 <kbd>Ctrl+Z</kbd>
                    </button>
                    <button type="button" disabled={disabled || !p.snapshot.canRedo} onClick={p.onRedo}>
                        重做 <kbd>Ctrl+Shift+Z</kbd>
                    </button>
                    <div className="menu-separator" />
                    <button type="button" disabled={disabled || !p.hasSelection} onClick={p.onEdit}>
                        编辑选中动作 <kbd>F2</kbd>
                    </button>
                    <button type="button" disabled={disabled} onClick={p.onWait}>
                        插入等待…
                    </button>
                    <button type="button" disabled={disabled} onClick={p.onText}>
                        插入文本…
                    </button>
                </div>
            </details>
            <details className="app-menu">
                <summary>视图</summary>
                <div className="menu-popup">
                    <button
                        type="button"
                        role="menuitemcheckbox"
                        aria-checked={p.actionsOpen}
                        onClick={p.onToggleActions}
                    >
                        <span>录制内容</span>
                        <span>{p.actionsOpen ? "✓" : ""}</span>
                    </button>
                    <button
                        type="button"
                        role="menuitemcheckbox"
                        aria-checked={p.timelineOpen}
                        onClick={p.onToggleTimeline}
                    >
                        <span>时间线</span>
                        <span>{p.timelineOpen ? "✓" : ""}</span>
                    </button>
                </div>
            </details>
            <details className="app-menu">
                <summary>帮助</summary>
                <div className="menu-popup">
                    <button type="button" disabled={disabled} onClick={p.onDemo}>
                        使用演示
                    </button>
                    <button type="button" onClick={p.onTour}>
                        界面导览
                    </button>
                    <div className="menu-separator" />
                    <button type="button" disabled={disabled} onClick={p.onShortcuts}>
                        快捷键设置…
                    </button>
                    <button type="button" onClick={p.onAbout}>
                        关于 Repeat
                    </button>
                </div>
            </details>
            <span className="app-caption">Repeat · 键鼠录制</span>
        </nav>
    );
}
