import { formatClock, vkName } from "../format";
import type { DocumentView, SessionSnapshot } from "../types";
import { Icon } from "./Icon";
interface Props {
    snapshot: SessionSnapshot;
    document: DocumentView;
    loopCount: string;
    busy: boolean;
    empty: boolean;
    actionsOpen: boolean;
    onLoopCountChange: (value: string) => void;
    onToggleActions: () => void;
    onStartRecording: () => void;
    onStopRecording: (gesture: string | null) => void;
    onCancelRecording: () => void;
    onStartExecution: () => void;
    onStopExecution: () => void;
    onReplay: () => void;
    onResetDemo: () => void;
    onRename: () => void;
    onDelete: () => void;
    onReference: () => void;
    onExitDemo: () => void;
}
export function TaskToolbar(p: Props) {
    const mode = p.snapshot.mode;
    const recording = mode === "recording";
    const executing = mode === "executing" || mode === "preparing";
    const demo = p.snapshot.isDemo;
    return (
        <header className="task-header" data-tour="record">
            {demo && (
                <button
                    type="button"
                    className="icon-button demo-back"
                    title="返回任务"
                    aria-label="返回任务"
                    onClick={p.onExitDemo}
                >
                    <Icon name="arrowLeft" size={18} />
                </button>
            )}
            <div className="task-heading">
                <h1 title={p.document.name}>{p.document.name}</h1>
                {demo && <span className="demo-caption">独立练习，不会改动你的任务</span>}
            </div>
            <div className="task-toolbar" role="toolbar" aria-label="当前任务操作">
                {demo ? (
                    <>
                        <button type="button" className="button primary" disabled title="使用演示中不录制系统键鼠操作">
                            <span className="record-dot" />
                            开始录制
                        </button>
                        <button
                            type="button"
                            className="button"
                            disabled={!p.document.actions.length}
                            onClick={p.onReplay}
                        >
                            <Icon name="play" />
                            回放
                        </button>
                        <span className="toolbar-separator" />
                        <button
                            type="button"
                            className="button subtle"
                            aria-expanded={p.actionsOpen}
                            onClick={p.onToggleActions}
                        >
                            <Icon name="layers" />
                            录制内容
                        </button>
                        <details className="app-menu task-more">
                            <summary aria-label="演示的更多操作">
                                <Icon name="more" />
                            </summary>
                            <div className="menu-popup">
                                <button type="button" onClick={p.onResetDemo}>
                                    重置演示
                                </button>
                            </div>
                        </details>
                    </>
                ) : recording ? (
                    <>
                        {p.snapshot.recordingLive && (
                            <span className="record-counter">{formatClock(p.snapshot.recordingLive.elapsedUs)}</span>
                        )}
                        <button
                            type="button"
                            className="button primary stop"
                            disabled={p.busy}
                            onClick={e => p.onStopRecording(e.detail > 0 ? "mouse" : null)}
                            onKeyDown={e => {
                                if (e.key === "Enter" || e.key === " ") {
                                    e.preventDefault();
                                    if (!e.repeat) p.onStopRecording(e.key === "Enter" ? "enter" : "space");
                                }
                            }}
                        >
                            <span className="record-dot stop-mark" />
                            停止录制
                        </button>
                        <button type="button" className="text-button" disabled={p.busy} onClick={p.onCancelRecording}>
                            放弃本次录制
                        </button>
                    </>
                ) : (
                    <>
                        <button
                            type="button"
                            className="button primary"
                            disabled={p.busy || executing || p.snapshot.hasPendingRecording}
                            title={`${vkName(p.snapshot.hotkeys.recordVk)} 启停录制`}
                            onClick={p.onStartRecording}
                            onKeyDown={e => {
                                if (e.key === "Enter" || e.key === " ") e.preventDefault();
                            }}
                            onKeyUp={e => {
                                if (e.key === "Enter" || e.key === " ") {
                                    e.preventDefault();
                                    p.onStartRecording();
                                }
                            }}
                        >
                            <span className="record-dot" />
                            开始录制
                        </button>
                        {!p.empty && (
                            <>
                                <span className="toolbar-separator" />
                                <label className="loop-field" title="每轮保留原节奏，轮间不额外等待">
                                    次数
                                    <input
                                        type="number"
                                        min={1}
                                        max={100000}
                                        step={1}
                                        value={p.loopCount}
                                        disabled={mode !== "idle"}
                                        onChange={e => p.onLoopCountChange(e.currentTarget.value)}
                                    />
                                </label>
                                {executing ? (
                                    <button type="button" className="button stop" onClick={p.onStopExecution}>
                                        <Icon name="pause" />
                                        {mode === "preparing" ? "取消执行" : "停止执行"}（
                                        {vkName(p.snapshot.hotkeys.stopVk)}）
                                    </button>
                                ) : (
                                    <button
                                        type="button"
                                        className="button execute"
                                        disabled={
                                            p.busy ||
                                            recording ||
                                            !p.document.events.length ||
                                            p.snapshot.hasPendingRecording
                                        }
                                        title="开始后直接执行当前录制的系统输入"
                                        onClick={p.onStartExecution}
                                    >
                                        <Icon name="play" />
                                        执行到系统
                                    </button>
                                )}
                                <span className="toolbar-separator" />
                                <button
                                    type="button"
                                    className="button subtle"
                                    aria-expanded={p.actionsOpen}
                                    onClick={p.onToggleActions}
                                >
                                    <Icon name="layers" />
                                    录制内容
                                </button>
                            </>
                        )}
                        <details className="app-menu task-more">
                            <summary aria-label="任务的更多操作">
                                <Icon name="more" />
                            </summary>
                            <div className="menu-popup">
                                <button type="button" disabled={p.busy || mode !== "idle"} onClick={p.onRename}>
                                    重命名任务
                                </button>
                                <button type="button" disabled={p.busy || mode !== "idle"} onClick={p.onReference}>
                                    {p.document.referenceImage ? "更换预览背景" : "添加预览背景"}
                                </button>
                                <div className="menu-separator" />
                                <button
                                    type="button"
                                    className="danger-text"
                                    disabled={p.busy || mode !== "idle"}
                                    onClick={p.onDelete}
                                >
                                    删除任务…
                                </button>
                            </div>
                        </details>
                    </>
                )}
            </div>
        </header>
    );
}
