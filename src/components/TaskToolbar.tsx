import { formatClock, stopKeyNames, vkName } from "../format";
import { repeatModeLabel, repeatSummary, type RepeatMode } from "../repeat";
import type { DocumentView, SessionSnapshot } from "../types";
import { Icon } from "./Icon";
import { SPEEDS } from "../speed";
interface Props {
    snapshot: SessionSnapshot;
    document: DocumentView;
    repeatMode: RepeatMode;
    loopCount: string;
    executionSpeed: number;
    busy: boolean;
    empty: boolean;
    onRepeatModeChange: (mode: RepeatMode) => void;
    onLoopCountChange: (value: string) => void;
    onExecutionSpeedChange: (speed: number) => void;
    onStartRecording: () => void;
    onStopRecording: (gesture: string | null) => void;
    onCancelRecording: () => void;
    onStartExecution: () => void;
    onReplay: () => void;
    onResetDemo: () => void;
    onExitDemo: () => void;
}
function RunMode(p: {
    mode: RepeatMode;
    value: string;
    disabled: boolean;
    onChangeMode: (mode: RepeatMode) => void;
    onChangeValue: (value: string) => void;
}) {
    const label = repeatModeLabel(p.mode, p.value);
    const summary = repeatSummary(p.mode, p.value);
    const options: { mode: RepeatMode; label: string; mark: string }[] = [
        { mode: "infinite", label: "无限循环", mark: "∞" },
        { mode: "once", label: "只执行一次", mark: "1" },
        { mode: "count", label: "自定义次数", mark: p.value || "—" },
    ];
    return (
        <details className="app-menu run-options">
            <summary className="run-count" aria-label={`重复方式：${label}`}>
                <Icon name="repeat" size={14} />
                {summary && <strong>{summary}</strong>}
            </summary>
            <div className="menu-popup run-mode-popup">
                <div className="run-mode-options" role="group" aria-label="重复方式">
                    {options.map(option => (
                        <button
                            key={option.mode}
                            type="button"
                            className="run-mode-option"
                            aria-pressed={p.mode === option.mode}
                            disabled={p.disabled}
                            onClick={() => p.onChangeMode(option.mode)}
                        >
                            <Icon name="repeat" size={14} />
                            <span>{option.label}</span>
                            <span className="run-mode-mark">{option.mark}</span>
                            <span className="run-mode-check" aria-hidden="true">
                                {p.mode === option.mode && <Icon name="check" size={14} />}
                            </span>
                        </button>
                    ))}
                </div>
                {p.mode === "count" && (
                    <label className="run-count-field">
                        执行次数
                        <input
                            type="number"
                            min={1}
                            max={100000}
                            step={1}
                            value={p.value}
                            autoFocus
                            disabled={p.disabled}
                            onChange={event => p.onChangeValue(event.currentTarget.value)}
                        />
                    </label>
                )}
            </div>
        </details>
    );
}
export function TaskToolbar(p: Props) {
    const mode = p.snapshot.mode;
    const recording = mode === "recording";
    const executing = mode === "executing" || mode === "preparing";
    const demo = p.snapshot.isDemo;
    return (
        <header className="task-header">
            <div className="task-header-left">
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
                </div>
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
                    </>
                ) : recording ? (
                    <>
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
                            title={
                                p.snapshot.recordHotkeyAvailable
                                    ? `${vkName(p.snapshot.hotkeys.recordVk)} 启停录制`
                                    : "录制快捷键不可用，请使用按钮或在帮助菜单更换快捷键"
                            }
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
                                {executing ? (
                                    <span className="button stop" role="status">
                                        <Icon name="pause" />
                                        {stopKeyNames(p.snapshot.hotkeys.stopVk, p.snapshot.stopHotkeyAvailable)} 停止
                                    </span>
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
                            </>
                        )}
                    </>
                )}
            </div>
            <div className="task-header-right">
                {demo ? (
                    <button type="button" className="text-button" onClick={p.onResetDemo}>
                        重置演示
                    </button>
                ) : recording && p.snapshot.recordingLive ? (
                    <span className="record-counter">{formatClock(p.snapshot.recordingLive.elapsedUs)}</span>
                ) : !p.empty && !recording && !executing ? (
                    <>
                        <select
                            className="speed-select"
                            aria-label="系统执行速度"
                            value={p.executionSpeed}
                            disabled={mode !== "idle"}
                            onChange={event => p.onExecutionSpeedChange(Number(event.currentTarget.value))}
                        >
                            {SPEEDS.map(speed => (
                                <option key={speed} value={speed}>
                                    {speed}x
                                </option>
                            ))}
                        </select>
                        <RunMode
                            mode={p.repeatMode}
                            value={p.loopCount}
                            disabled={mode !== "idle"}
                            onChangeMode={p.onRepeatModeChange}
                            onChangeValue={p.onLoopCountChange}
                        />
                    </>
                ) : null}
            </div>
        </header>
    );
}
