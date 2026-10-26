import type { SessionSnapshot } from "../types";
import { Icon } from "./Icon";
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
    onUndo: () => void;
    onRedo: () => void;
    onEdit: () => void;
    onWait: () => void;
    onText: () => void;
    onDemo: () => void;
    onTour: () => void;
    onShortcuts: () => void;
    onAbout: () => void;
}
export function MenuBar(p: Props) {
    const disabled = p.busy || p.snapshot.mode !== "idle";
    return (
        <>
            <div className="brand">
                <Icon name="repeat" size={21} />
                <strong>Repeat</strong>
            </div>
            <nav className="app-menubar" aria-label="应用菜单">
                <details className="app-menu">
                    <summary>文件</summary>
                    <div className="menu-popup">
                        <button type="button" disabled={disabled || p.snapshot.isDemo} onClick={p.onCreate}>
                            新建任务 <kbd>Ctrl N</kbd>
                        </button>
                        <button type="button" disabled={disabled || p.snapshot.isDemo} onClick={p.onRename}>
                            重命名任务
                        </button>
                    </div>
                </details>
                <details className="app-menu">
                    <summary>编辑</summary>
                    <div className="menu-popup">
                        <button type="button" disabled={disabled || !p.snapshot.canUndo} onClick={p.onUndo}>
                            撤销 <kbd>Ctrl Z</kbd>
                        </button>
                        <button type="button" disabled={disabled || !p.snapshot.canRedo} onClick={p.onRedo}>
                            重做 <kbd>Ctrl Shift Z</kbd>
                        </button>
                        <div className="menu-separator" />
                        <button type="button" disabled={disabled || !p.hasSelection} onClick={p.onEdit}>
                            编辑选中动作
                        </button>
                        <button type="button" disabled={disabled || !p.hasSelection} onClick={p.onWait}>
                            插入等待
                        </button>
                        <button type="button" disabled={disabled} onClick={p.onText}>
                            插入文本…
                        </button>
                    </div>
                </details>
                <details className="app-menu">
                    <summary>视图</summary>
                    <div className="menu-popup">
                        <button type="button" aria-expanded={p.actionsOpen} onClick={p.onToggleActions}>
                            录制内容
                        </button>
                        <button type="button" aria-expanded={p.timelineOpen} onClick={p.onToggleTimeline}>
                            时间线
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
            </nav>
            <span className="app-caption">键鼠录制</span>
        </>
    );
}
