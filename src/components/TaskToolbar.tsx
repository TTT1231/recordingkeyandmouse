import { formatDurationUs, vkName } from "../format";
import type { DocumentView, SessionSnapshot } from "../types";
import { Icon } from "./Icon";
interface Props {
    snapshot: SessionSnapshot;
    document: DocumentView;
    loopCount: string;
    busy: boolean;
    onLoopCountChange: (value: string) => void;
    onStartRecording: () => void;
    onStopRecording: (gesture: string | null) => void;
    onCancelRecording: () => void;
    onStartExecution: () => void;
    onStopExecution: () => void;
    onRename: () => void;
    onDelete: () => void;
    onReference: () => void;
    onShowReport: () => void;
    onExitDemo: () => void;
}
export function TaskToolbar(p: Props) {
    const mode = p.snapshot.mode;
    const recording = mode === "recording";
    const executing = mode === "executing" || mode === "preparing";
    return (
        <header className="task-header" data-tour="record">
            <div className="task-heading">
                <h1 title={p.document.name}>{p.document.name}</h1>
                <span className="save-status">
                    {p.snapshot.isDemo
                        ? "使用演示"
                        : recording
                          ? "录制结束后保存"
                          : p.snapshot.hasPendingRecording
                            ? "录制尚未保存"
                            : "已保存"}
                </span>
            </div>
            <div className="task-toolbar" role="toolbar" aria-label="当前任务操作">
                {p.snapshot.isDemo ? (
                    <button type="button" className="button primary" onClick={p.onExitDemo}>
                        退出演示
                    </button>
                ) : recording ? (
                    <>
                        {p.snapshot.recordingLive && (
                            <span className="record-counter">
                                {formatDurationUs(p.snapshot.recordingLive.elapsedUs)} · 录制中
                            </span>
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
                )}
                <span className="toolbar-separator" />
                <label className="loop-field" title="每轮保留原节奏，轮间不额外等待">
                    次数
                    <input
                        type="number"
                        min={1}
                        max={100000}
                        step={1}
                        value={p.loopCount}
                        disabled={mode !== "idle" || p.snapshot.isDemo}
                        onChange={e => p.onLoopCountChange(e.currentTarget.value)}
                    />
                </label>
                {executing ? (
                    <button type="button" className="button stop" onClick={p.onStopExecution}>
                        <Icon name="pause" />
                        {mode === "preparing" ? "取消执行" : "停止执行"}（{vkName(p.snapshot.hotkeys.stopVk)}）
                    </button>
                ) : (
                    <button
                        type="button"
                        className="button execute"
                        disabled={
                            p.busy ||
                            recording ||
                            p.snapshot.isDemo ||
                            !p.document.events.length ||
                            p.snapshot.hasPendingRecording
                        }
                        title="开始后有 3 秒准备时间，请切换到目标应用"
                        onClick={p.onStartExecution}
                    >
                        <Icon name="play" />
                        执行到系统
                    </button>
                )}
                <details className="app-menu task-more">
                    <summary aria-label="任务的更多操作">
                        <Icon name="more" />
                    </summary>
                    <div className="menu-popup">
                        <button
                            type="button"
                            disabled={p.busy || mode !== "idle" || p.snapshot.isDemo}
                            onClick={p.onRename}
                        >
                            重命名任务
                        </button>
                        <button type="button" disabled={p.busy || mode !== "idle"} onClick={p.onReference}>
                            {p.document.referenceImage ? "更换预览参考图" : "添加预览参考图"}
                        </button>
                        <button type="button" disabled={!p.snapshot.hasExecutionReport} onClick={p.onShowReport}>
                            上次执行报告
                        </button>
                        <div className="menu-separator" />
                        <button
                            type="button"
                            className="danger-text"
                            disabled={p.busy || mode !== "idle" || p.snapshot.isDemo}
                            onClick={p.onDelete}
                        >
                            删除任务…
                        </button>
                    </div>
                </details>
            </div>
            <span className={`mode-badge mode-${mode}`}>
                {recording
                    ? "录制中"
                    : mode === "preparing"
                      ? "准备执行"
                      : executing
                        ? "执行中"
                        : p.snapshot.isDemo
                          ? "演示"
                          : "空闲"}
            </span>
        </header>
    );
}
