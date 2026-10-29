import { useCallback, useEffect, useRef, useState, type CSSProperties } from "react";
import { createPortal } from "react-dom";
import * as api from "./api";
import { actionAtTime } from "./actions";
import { ActionsPanel } from "./components/ActionsPanel";
import { EditorDialog, Modal, type EditorTarget } from "./components/EditorDialog";
import { SidebarResizer } from "./components/SidebarResizer";
import { TaskSidebar } from "./components/TaskSidebar";
import { TaskToolbar } from "./components/TaskToolbar";
import { Timeline } from "./components/Timeline";
import { TitleBar, WindowControls } from "./components/TitleBar";
import { ViewerArea } from "./components/ViewerArea";
import { WindowResizeHandles } from "./components/WindowResizeHandles";
import { INFINITE_ROUNDS, repeatRounds, type RepeatMode } from "./repeat";
import { NameDialog, ReferenceDialog, ShortcutsDialog } from "./components/WorkspaceDialogs";
import { stopKeyNames, vkName } from "./format";
import { SIDEBAR_DEFAULT, SIDEBAR_OPEN_KEY, SIDEBAR_WIDTH_KEY, clampSidebarWidth } from "./layout";
import type { ActionView, EditRequest, SessionSnapshot, WorkspaceView } from "./types";
import { version } from "../package.json";
import appIcon from "../src-tauri/icons/128x128.png";
import "./App.css";
type Dialog =
    | { type: "editor"; target: EditorTarget }
    | { type: "record" | "create" | "rename" | "delete" | "reference" | "shortcuts" | "about" }
    | null;
function readSidebarWidth(): number {
    try {
        const stored = Number(window.localStorage.getItem(SIDEBAR_WIDTH_KEY));
        return clampSidebarWidth(Number.isFinite(stored) && stored > 0 ? stored : SIDEBAR_DEFAULT, window.innerWidth);
    } catch {
        return clampSidebarWidth(SIDEBAR_DEFAULT, window.innerWidth);
    }
}
function readSidebarOpen(): boolean {
    try {
        return window.localStorage.getItem(SIDEBAR_OPEN_KEY) !== "0";
    } catch {
        return true;
    }
}
export default function App() {
    const [workspace, setWorkspace] = useState<WorkspaceView | null>(null),
        [snapshot, setSnapshot] = useState<SessionSnapshot | null>(null),
        [error, setError] = useState<string | null>(null),
        [notice, setNotice] = useState<string | null>(null),
        [busy, setBusy] = useState(false);
    const [timeUs, setTimeUs] = useState(0),
        [playing, setPlaying] = useState(false),
        [speed, setSpeed] = useState(1),
        [executionSpeed, setExecutionSpeed] = useState(1),
        [selectedId, setSelectedId] = useState<string | null>(null),
        [loopCount, setLoopCount] = useState("10"),
        [repeatMode, setRepeatMode] = useState<RepeatMode>("once");
    const [actionsOpen, setActionsOpen] = useState(true),
        [timelineOpen, setTimelineOpen] = useState(true),
        [sidebarOpen, setSidebarOpen] = useState(readSidebarOpen),
        [sidebarWidth, setSidebarWidth] = useState(readSidebarWidth),
        [dialog, setDialog] = useState<Dialog>(null),
        [picking, setPicking] = useState(false),
        [pickedPoint, setPickedPoint] = useState<{ x: number; y: number } | null>(null),
        [dialogNode, setDialogNode] = useState<HTMLDialogElement | null>(null),
        [toast, setToast] = useState("");
    const workspaceRef = useRef<WorkspaceView | null>(null),
        snapshotRef = useRef<SessionSnapshot | null>(null),
        previousDemoPanels = useRef<{ actionsOpen: boolean; timelineOpen: boolean } | null>(null),
        mounted = useRef(true),
        fetchSerial = useRef(0),
        mutating = useRef(false);
    const acceptSnapshot = useCallback((next: SessionSnapshot) => {
        if (!mounted.current || next.version < (snapshotRef.current?.version ?? 0)) return false;
        snapshotRef.current = next;
        setSnapshot(next);
        return true;
    }, []);
    const acceptWorkspace = useCallback(
        (next: WorkspaceView) => {
            if (!mounted.current) return;
            const latest = snapshotRef.current;
            if (
                latest &&
                next.session.version < latest.version &&
                (next.document.taskId !== latest.taskId || next.document.revision !== latest.revision)
            )
                return;
            const previous = workspaceRef.current;
            if (previous?.document.taskId !== next.document.taskId) {
                setTimeUs(0);
                setPlaying(false);
                setSelectedId(null);
            } else if (previous.document.revision !== next.document.revision) {
                setTimeUs(t => Math.min(t, next.document.durationUs));
                setPlaying(false);
                setSelectedId(id => (next.document.actions.some(a => a.id === id) ? id : null));
            }
            workspaceRef.current = next;
            setWorkspace(next);
            acceptSnapshot(next.session);
        },
        [acceptSnapshot],
    );
    const refresh = useCallback(async () => {
        const serial = ++fetchSerial.current;
        try {
            const next = await api.fetchWorkspace();
            if (serial === fetchSerial.current) acceptWorkspace(next);
        } catch (e) {
            if (mounted.current) {
                if (workspaceRef.current) setNotice(String(e));
                else setError(String(e));
            }
        }
    }, [acceptWorkspace]);
    useEffect(() => {
        mounted.current = true;
        const subscription = api
            .onStateChanged(next => {
                if (!acceptSnapshot(next)) return;
                const doc = workspaceRef.current?.document;
                if (!doc || doc.taskId !== next.taskId || doc.revision !== next.revision) void refresh();
                if (next.mode !== "idle") {
                    setPlaying(false);
                    setDialog(current => (current?.type === "about" ? current : null));
                    setPicking(false);
                }
            })
            .catch(e => {
                if (mounted.current) setNotice(String(e));
                return () => {};
            });
        void refresh();
        return () => {
            mounted.current = false;
            void subscription.then(stop => stop());
        };
    }, [acceptSnapshot, refresh]);
    useEffect(() => {
        if (!snapshot || snapshot.mode === "idle") return;
        const timer = setInterval(
            () =>
                void api
                    .fetchStatus()
                    .then(acceptSnapshot)
                    .catch(e => setNotice(String(e))),
            250,
        );
        return () => clearInterval(timer);
    }, [snapshot?.mode, acceptSnapshot]);
    useEffect(() => {
        if (!toast) return;
        const timer = setTimeout(() => setToast(""), 2600);
        return () => clearTimeout(timer);
    }, [toast]);
    const lastError = (snapshot ?? workspace?.session)?.lastError;
    const formDialog = !!dialog && dialog.type !== "about";
    useEffect(() => {
        setDialogNode(globalThis.document.querySelector<HTMLDialogElement>("dialog.dialog[open]"));
    }, [dialog, picking]);
    useEffect(() => {
        if (!workspace || !error || formDialog) return;
        const timer = window.setTimeout(() => setError(current => (current === error ? null : current)), 5000);
        return () => window.clearTimeout(timer);
    }, [!!workspace, error, formDialog]);
    useEffect(() => {
        if (!notice) return;
        const timer = window.setTimeout(() => setNotice(current => (current === notice ? null : current)), 5000);
        return () => window.clearTimeout(timer);
    }, [notice]);
    useEffect(() => {
        if (!workspace || !lastError) return;
        const timer = window.setTimeout(() => {
            if (snapshotRef.current?.lastError === lastError) void api.clearError().catch(() => {});
        }, 5000);
        return () => window.clearTimeout(timer);
    }, [!!workspace, lastError]);
    useEffect(() => {
        const timer = window.setTimeout(() => {
            try {
                window.localStorage.setItem(SIDEBAR_WIDTH_KEY, String(sidebarWidth));
            } catch {
                // 本地存储不可用时宽度只在本次会话内生效。
            }
        }, 200);
        return () => window.clearTimeout(timer);
    }, [sidebarWidth]);
    useEffect(() => {
        try {
            window.localStorage.setItem(SIDEBAR_OPEN_KEY, sidebarOpen ? "1" : "0");
        } catch {
            // 同上。
        }
    }, [sidebarOpen]);
    useEffect(() => {
        const keepInBounds = () => setSidebarWidth(current => clampSidebarWidth(current, window.innerWidth));
        window.addEventListener("resize", keepInBounds);
        return () => window.removeEventListener("resize", keepInBounds);
    }, []);
    useEffect(() => {
        const close = (event: MouseEvent) => {
            const target = event.target instanceof Element ? event.target : null;
            // 菜单/重复方式弹层互斥：打开一个时关闭其余，避免叠在一起。
            const summary = target?.closest("summary");
            if (summary?.parentElement instanceof HTMLDetailsElement) {
                globalThis.document.querySelectorAll<HTMLDetailsElement>("details.app-menu[open]").forEach(node => {
                    if (node !== summary.parentElement) node.open = false;
                });
                return;
            }
            if (target?.closest("details.run-options")) return;
            if (target?.closest("details.app-menu") && !target.closest("button")) return;
            globalThis.document.querySelectorAll<HTMLDetailsElement>("details.app-menu[open]").forEach(node => {
                node.open = false;
            });
        };
        window.addEventListener("click", close);
        return () => window.removeEventListener("click", close);
    }, []);
    const run = useCallback(
        async (operation: () => Promise<WorkspaceView | SessionSnapshot | void>, quiet = false) => {
            if (mutating.current) return false;
            mutating.current = true;
            if (!quiet) setBusy(true);
            setError(null);
            try {
                const result = await operation();
                if (result && "document" in result) acceptWorkspace(result);
                else if (result) acceptSnapshot(result);
                return true;
            } catch (e) {
                setError(String(e));
                return false;
            } finally {
                mutating.current = false;
                if (!quiet && mounted.current) setBusy(false);
            }
        },
        [acceptWorkspace, acceptSnapshot],
    );
    const openDialog = useCallback((next: Dialog) => {
        setPlaying(false);
        if (next?.type !== "about") setError(null);
        setPicking(false);
        setPickedPoint(null);
        setDialog(next);
    }, []);
    const closeDialog = useCallback(() => {
        setDialog(null);
        setPicking(false);
        setPickedPoint(null);
        if (formDialog) setError(null);
    }, [formDialog]);
    const document = workspace?.document;
    const session = snapshot ?? workspace?.session;
    const mode = session?.mode ?? "idle";
    const locked = busy || mode !== "idle";
    const rows = document?.actions ?? [];
    const empty = mode === "idle" && !document?.actions.length && (document?.durationUs ?? 0) <= 0 && !session?.isDemo;
    const selected = rows.find(row => row.id === selectedId) ?? null;
    const selectAction = useCallback((action: ActionView) => {
        setSelectedId(action.id);
        setTimeUs(action.startUs);
        setPlaying(false);
    }, []);
    const editAction = useCallback(
        (action: ActionView) => {
            const all = workspaceRef.current?.document.actions ?? [];
            const next = all.find(a => a.startUs >= action.endUs && a.id !== action.id);
            selectAction(action);
            openDialog({
                type: "editor",
                target: { action, after: action.id, linkAvailable: next?.kind.type === "click" && next.editable },
            });
        },
        [selectAction, openDialog],
    );
    const insert = useCallback(
        (type: "wait" | "text", after: string | null) =>
            openDialog({ type: "editor", target: { insert: type, after, linkAvailable: false } }),
        [openDialog],
    );
    const saveEdit = useCallback(
        async (edit: EditRequest, showFeedback = true) => {
            const doc = workspaceRef.current?.document;
            if (!doc) return;
            const before = new Set(doc.actions.map(a => a.id));
            if (!showFeedback) setToast("");
            if (await run(() => api.editTask(doc, edit))) {
                closeDialog();
                if (showFeedback) setToast(snapshotRef.current?.isDemo ? "演示中的修改已生效" : "已保存");
                const added = workspaceRef.current?.document.actions.find(a => !before.has(a.id));
                if (added) selectAction(added);
            }
        },
        [run, closeDialog, selectAction],
    );
    const changeHistory = useCallback(
        (redo: boolean) => {
            const doc = workspaceRef.current?.document;
            if (doc) void run(() => api.taskHistory(doc, redo));
        },
        [run],
    );
    const saveTask = useCallback(async () => {
        if (snapshotRef.current?.hasPendingRecording) {
            await run(() => api.retryRecording());
            return;
        }
        if (await run(api.fetchWorkspace)) setToast("所有修改已保存到此设备");
    }, [run]);
    useEffect(() => {
        const key = (event: KeyboardEvent) => {
            if (
                (event.target instanceof Element &&
                    event.target.closest("input,textarea,select,[contenteditable=true]")) ||
                dialog ||
                locked
            )
                return;
            if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "n") {
                event.preventDefault();
                if (!session?.isDemo) openDialog({ type: "create" });
            }
            if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
                event.preventDefault();
                void saveTask();
            }
            if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") {
                event.preventDefault();
                if (event.shiftKey ? snapshotRef.current?.canRedo : snapshotRef.current?.canUndo)
                    changeHistory(event.shiftKey);
            }
            if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "y") {
                event.preventDefault();
                if (snapshotRef.current?.canRedo) changeHistory(true);
            }
            if (event.key === "F2" && selected) {
                event.preventDefault();
                editAction(selected);
            }
            if (event.code === "Space") {
                if (
                    event.target instanceof Element &&
                    event.target.closest("button,summary,a,[role=button],[role=option]")
                )
                    return;
                event.preventDefault();
                if (playing) setPlaying(false);
                else {
                    if (timeUs >= (document?.durationUs ?? 0)) setTimeUs(0);
                    setPlaying(true);
                }
            }
        };
        window.addEventListener("keydown", key);
        return () => window.removeEventListener("keydown", key);
    }, [
        dialog,
        locked,
        saveTask,
        changeHistory,
        selected,
        editAction,
        openDialog,
        session?.isDemo,
        playing,
        timeUs,
        document?.durationUs,
    ]);
    // 执行中的播放头：快照每 100–250ms 才到一次，直接驱动时间轴会一顿一顿地跳。
    // 以最近一次快照为基准本地外推，两份快照之间由 rAF 平滑推进；回放节奏仍由 Rust 掌控。
    const execBase = useRef<{ atMs: number; elapsedUs: number } | null>(null);
    useEffect(() => {
        const live = session?.executionLive;
        if (mode !== "executing" || !live) {
            execBase.current = null;
            return;
        }
        execBase.current = { atMs: performance.now(), elapsedUs: live.elapsedUs };
    }, [mode, session?.executionLive?.elapsedUs, session?.executionLive?.currentRound]);
    useEffect(() => {
        if (mode !== "executing") return;
        let raf = 0;
        const tick = () => {
            const base = execBase.current;
            if (base)
                setTimeUs(
                    Math.min(
                        document?.durationUs ?? 0,
                        base.elapsedUs + (performance.now() - base.atMs) * 1000 * executionSpeed,
                    ),
                );
            raf = requestAnimationFrame(tick);
        };
        raf = requestAnimationFrame(tick);
        return () => cancelAnimationFrame(raf);
    }, [mode, document?.durationUs, executionSpeed]);
    const finishPlay = useCallback(() => setPlaying(false), []);
    const togglePlay = useCallback(() => {
        if (playing) {
            setPlaying(false);
            return;
        }
        if (timeUs >= (workspaceRef.current?.document.durationUs ?? 0)) setTimeUs(0);
        setPlaying(true);
    }, [playing, timeUs]);
    if (!workspace || !document || !session)
        return (
            <div className="app-shell">
                <header className="app-titlebar">
                    <div className="titlebar-drag" data-tauri-drag-region aria-hidden="true" />
                    <WindowControls />
                </header>
                <div className="startup-screen">
                    <h1>Repeat</h1>
                    <p>{error ? "本地任务未能载入" : "正在载入本地任务…"}</p>
                    {error && (
                        <>
                            <p role="alert">{error}</p>
                            <button type="button" className="button" onClick={() => void refresh()}>
                                重试
                            </button>
                        </>
                    )}
                </div>
            </div>
        );
    const shownDocument =
        mode === "recording"
            ? {
                  ...document,
                  durationUs: session.recordingLive?.elapsedUs ?? 0,
                  events: [],
                  actions: [],
                  referenceImage: null,
              }
            : document;
    const notices = [!formDialog ? error : null, notice, session.lastError].filter(
        (message, index, messages): message is string => !!message && messages.indexOf(message) === index,
    );
    const errorNotification = notices.length > 0 && (
        <div className="error-bar" role="alert">
            <span>
                {notices.map(message => (
                    <div key={message}>{message}</div>
                ))}
            </span>
            <button
                type="button"
                className="text-button"
                aria-label="关闭错误提示"
                onClick={() => {
                    if (!formDialog) setError(null);
                    setNotice(null);
                    if (session.lastError) void api.clearError().catch(e => setNotice(String(e)));
                }}
            >
                ×
            </button>
        </div>
    );
    return (
        <div className="app-shell">
            <TitleBar
                snapshot={session}
                busy={busy}
                actionsOpen={actionsOpen}
                timelineOpen={timelineOpen}
                sidebarOpen={sidebarOpen}
                onToggleSidebar={() => setSidebarOpen(value => !value)}
                onToggleActions={() => setActionsOpen(value => !value)}
                onToggleTimeline={() => setTimelineOpen(value => !value)}
                onCreate={() => openDialog({ type: "create" })}
                onRename={() => openDialog({ type: "rename" })}
                onUndo={() => changeHistory(false)}
                onRedo={() => changeHistory(true)}
                onDemo={async () => {
                    setPlaying(false);
                    if (await run(() => api.demoMode(true))) {
                        previousDemoPanels.current = { actionsOpen, timelineOpen };
                        setActionsOpen(true);
                        setTimelineOpen(true);
                    }
                }}
                onShortcuts={() => openDialog({ type: "shortcuts" })}
                onAbout={() => openDialog({ type: "about" })}
            />
            {dialogNode ? createPortal(errorNotification, dialogNode) : errorNotification}
            {session.hasPendingRecording && (
                <div className="pending-notice">
                    原任务保持不变，新录制尚未保存。
                    <button
                        type="button"
                        className="text-button"
                        disabled={busy}
                        onClick={() => void run(() => api.retryRecording())}
                    >
                        重试保存
                    </button>
                    <button
                        type="button"
                        className="text-button"
                        disabled={busy}
                        onClick={() => void run(() => api.retryRecording(true))}
                    >
                        放弃本次录制
                    </button>
                </div>
            )}
            <div
                className={`app-layout${sidebarOpen ? "" : " sidebar-collapsed"}`}
                style={{ "--sidebar-width": `${sidebarWidth}px` } as CSSProperties}
            >
                <TaskSidebar
                    snapshot={session}
                    document={document}
                    tasks={workspace.tasks}
                    disabled={locked}
                    collapsed={!sidebarOpen}
                    onCreate={() => openDialog({ type: "create" })}
                    onSelect={id => {
                        if (id === session.taskId) return;
                        void run(() => api.selectTask(id), true);
                    }}
                    onRename={() => openDialog({ type: "rename" })}
                    onDelete={() => openDialog({ type: "delete" })}
                />
                <SidebarResizer width={sidebarWidth} collapsed={!sidebarOpen} onChange={setSidebarWidth} />
                <main className={`task-workspace${empty ? " empty-task" : ""}`}>
                    <TaskToolbar
                        snapshot={session}
                        document={document}
                        repeatMode={repeatMode}
                        loopCount={loopCount}
                        executionSpeed={executionSpeed}
                        busy={busy}
                        empty={empty}
                        onRepeatModeChange={setRepeatMode}
                        onLoopCountChange={setLoopCount}
                        onExecutionSpeedChange={setExecutionSpeed}
                        onStartRecording={() => openDialog({ type: "record" })}
                        onStopRecording={gesture => void run(() => api.stopRecording(gesture))}
                        onCancelRecording={() => void run(() => api.cancelRecording(document))}
                        onStartExecution={() => {
                            const rounds = repeatRounds(repeatMode, loopCount);
                            if (rounds === null) {
                                setError("执行次数应为 1–100000 的整数");
                                return;
                            }
                            setPlaying(false);
                            void run(() => api.startExecution(document, rounds, executionSpeed));
                        }}
                        onReplay={() => {
                            setPlaying(false);
                            setTimeUs(0);
                            setPlaying(true);
                        }}
                        onResetDemo={() => void run(() => api.demoMode(true))}
                        onExitDemo={async () => {
                            if (await run(() => api.demoMode(false))) {
                                const previous = previousDemoPanels.current;
                                if (previous) {
                                    setActionsOpen(previous.actionsOpen);
                                    setTimelineOpen(previous.timelineOpen);
                                    previousDemoPanels.current = null;
                                }
                            }
                        }}
                    />
                    {mode === "preparing" && (
                        <div className="executing-notice" role="status">
                            将在 {Math.max(1, Math.ceil((session.executionLive?.prepareRemainingMs ?? 0) / 1000))}{" "}
                            秒后执行；{stopKeyNames(session.hotkeys.stopVk, session.stopHotkeyAvailable)} 可取消。
                        </div>
                    )}
                    {mode === "executing" && session.executionLive && (
                        <div className="executing-notice" role="status">
                            {session.executionLive.roundsRequested === INFINITE_ROUNDS
                                ? `无限循环 · 第 ${session.executionLive.currentRound} 轮`
                                : `第 ${session.executionLive.currentRound} / ${session.executionLive.roundsRequested} 轮`}
                            {" · "}已执行 {session.executionLive.eventsInjected} 个输入
                            {session.executionLive.stopRequested ? " · 正在停止…" : ""}
                        </div>
                    )}
                    {picking && (
                        <div className="pick-notice">
                            点击预览设置坐标。
                            <button type="button" className="text-button" onClick={() => setPicking(false)}>
                                返回编辑窗口
                            </button>
                        </div>
                    )}
                    <div className="workspace-body">
                        <div className="viewer-column">
                            <ViewerArea
                                recording={shownDocument}
                                rows={shownDocument.actions}
                                timeUs={mode === "recording" ? 0 : timeUs}
                                playing={playing}
                                speed={speed}
                                disabled={mode !== "idle" || picking}
                                picking={picking}
                                empty={empty}
                                isDemo={session.isDemo}
                                onTimeChange={setTimeUs}
                                onTogglePlay={togglePlay}
                                onEnd={finishPlay}
                                onSpeedChange={setSpeed}
                                onSelectAction={selectAction}
                                onPick={(x, y) => {
                                    setPickedPoint({ x, y });
                                    setPicking(false);
                                }}
                                onAddReference={() => openDialog({ type: "reference" })}
                                timelineOpen={timelineOpen}
                                onCloseTimeline={() => setTimelineOpen(false)}
                            />
                        </div>
                        {
                            <ActionsPanel
                                hidden={!actionsOpen || empty}
                                rows={shownDocument.actions}
                                selectedId={selectedId}
                                recordingLive={mode === "recording"}
                                disabled={locked}
                                onSelect={selectAction}
                                onEdit={editAction}
                                onInsertWait={after => insert("wait", after)}
                                onInsertText={after => insert("text", after)}
                                onDeleteAction={id => void saveEdit({ op: "deleteAction", actionId: id })}
                                onCollapse={() => setActionsOpen(false)}
                            />
                        }
                    </div>
                    {
                        <Timeline
                            hidden={!timelineOpen || empty}
                            recording={shownDocument}
                            rows={shownDocument.actions}
                            timeUs={mode === "recording" ? 0 : timeUs}
                            selectedId={selectedId}
                            disabled={locked}
                            onUpdateWait={(actionId, durationUs) =>
                                void saveEdit({ op: "updateWait", actionId, durationUs }, false)
                            }
                            onSeek={(time, action) => {
                                setPlaying(false);
                                setTimeUs(time);
                                setSelectedId(action?.id ?? actionAtTime(rows, time)?.id ?? null);
                            }}
                        />
                    }
                </main>
            </div>
            <WindowResizeHandles />
            {toast && (
                <div className="toast" role="status">
                    {toast}
                </div>
            )}
            {dialog?.type === "record" && (
                <Modal title="开始录制" onClose={closeDialog}>
                    <div className="dialog-body">
                        <p className="dialog-lead">
                            {session.recordHotkeyAvailable ? (
                                <>
                                    像平常一样操作，按 <kbd className="keycap">{vkName(session.hotkeys.recordVk)}</kbd>{" "}
                                    或点击停止按钮结束录制。
                                </>
                            ) : (
                                <>录制快捷键暂不可用，请点击停止按钮结束录制。可在帮助菜单的快捷键设置中更换快捷键。</>
                            )}
                        </p>
                        {document.actions.length > 0 && <p className="form-note">本次录制将替换现有录制。</p>}
                        {error && (
                            <p className="form-error" role="alert">
                                {error}
                            </p>
                        )}
                    </div>
                    <div className="dialog-actions">
                        <button type="button" className="button" onClick={closeDialog}>
                            取消
                        </button>
                        <button
                            type="button"
                            className="button primary"
                            disabled={locked}
                            onClick={async () => {
                                if (await run(() => api.startRecording(document))) closeDialog();
                            }}
                        >
                            <span className="record-dot" />
                            开始
                        </button>
                    </div>
                </Modal>
            )}
            {dialog?.type === "editor" && (
                <EditorDialog
                    target={dialog.target}
                    busy={locked}
                    error={error}
                    picking={picking}
                    pickedPoint={pickedPoint}
                    onPick={() => {
                        setPlaying(false);
                        setPicking(true);
                    }}
                    onClose={closeDialog}
                    onSave={edit => void saveEdit(edit)}
                />
            )}
            {(dialog?.type === "create" || dialog?.type === "rename") && (
                <NameDialog
                    mode={dialog.type}
                    initial={dialog.type === "create" ? "新任务" : document.name}
                    busy={locked}
                    error={error}
                    onClose={closeDialog}
                    onSave={async name => {
                        if (
                            await run(() =>
                                dialog.type === "create" ? api.createTask(name) : api.renameTask(document, name),
                            )
                        )
                            closeDialog();
                    }}
                />
            )}
            {dialog?.type === "delete" && (
                <Modal title="删除任务" onClose={closeDialog}>
                    <div className="delete-dialog-body">
                        <p>删除后，录制内容、编辑结果和预览背景将一并删除，此操作无法撤销。</p>
                        {error && (
                            <p className="form-error" role="alert">
                                {error}
                            </p>
                        )}
                    </div>
                    <div className="dialog-actions">
                        <button type="button" className="button" onClick={closeDialog} autoFocus>
                            取消
                        </button>
                        <button
                            type="button"
                            className="button danger"
                            disabled={locked}
                            onClick={async () => {
                                if (await run(() => api.deleteTask(document))) closeDialog();
                            }}
                        >
                            删除任务
                        </button>
                    </div>
                </Modal>
            )}
            {dialog?.type === "reference" && (
                <ReferenceDialog
                    document={document}
                    busy={locked}
                    error={error}
                    onClose={closeDialog}
                    onSave={reference => void saveEdit({ op: "setReference", reference })}
                />
            )}
            {dialog?.type === "shortcuts" && (
                <ShortcutsDialog
                    hotkeys={session.hotkeys}
                    recordHotkeyAvailable={session.recordHotkeyAvailable}
                    stopHotkeyAvailable={session.stopHotkeyAvailable}
                    busy={locked}
                    error={error}
                    onClose={closeDialog}
                    onSave={async hotkeys => {
                        if (await run(() => api.setHotkeys(hotkeys))) closeDialog();
                    }}
                />
            )}
            {dialog?.type === "about" && (
                <Modal title="关于 Repeat" onClose={closeDialog}>
                    <div className="about-dialog-body">
                        <div className="about-product">
                            <img src={appIcon} alt="" width="64" height="64" />
                            <div>
                                <p className="about-name">
                                    Repeat <span className="about-version">{version}</span>
                                </p>
                                <p className="about-category">桌面键鼠录制与回放工具</p>
                            </div>
                        </div>
                        <p className="about-description">录制一次，按需要编辑，再预览或执行重复操作。</p>
                    </div>
                    <div className="dialog-actions">
                        <button type="button" className="button" onClick={closeDialog}>
                            关闭
                        </button>
                    </div>
                </Modal>
            )}
        </div>
    );
}
