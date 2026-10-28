export type MouseButton = "left" | "right" | "middle" | "x1" | "x2";
export type WheelAxis = "vertical" | "horizontal";
export type EventKind =
    | { kind: "keyDown" | "keyUp"; vk: number; scan: number; extended: boolean }
    | { kind: "mouseMove"; x: number; y: number }
    | { kind: "mouseDown" | "mouseUp"; button: MouseButton; x: number; y: number }
    | { kind: "wheel"; axis: WheelAxis; delta: number; x: number; y: number }
    | { kind: "unicode"; unit: number; up: boolean };
export interface RecordedEvent {
    seq: number;
    timeUs: number;
    nativeTimeMs: number;
    kind: EventKind;
}
export interface MonitorInfo {
    name: string;
    x: number;
    y: number;
    width: number;
    height: number;
    dpi: number;
}
export interface DesktopGeometry {
    monitors: MonitorInfo[];
}
export type ActionKind =
    | { type: "move"; x: number; y: number }
    | { type: "click"; button: MouseButton; count: number; x: number; y: number }
    | { type: "drag"; button: MouseButton; x: number; y: number }
    | { type: "wheel"; axis: WheelAxis; delta: number; x: number; y: number }
    | { type: "keys"; chord: string }
    | { type: "text"; text: string }
    | { type: "wait" };
export interface ActionView {
    id: string;
    startUs: number;
    endUs: number;
    kind: ActionKind;
    title: string;
    detail: string;
    eventCount: number;
    editable: boolean;
}
export interface ImageMapping {
    x: number;
    y: number;
    width: number;
    height: number;
}
export interface ReferenceImage {
    name: string;
    dataUrl: string;
    width: number;
    height: number;
    mapping: ImageMapping;
}
export interface DocumentView {
    taskId: string;
    name: string;
    revision: number;
    durationUs: number;
    geometry: DesktopGeometry;
    layoutKlid: string;
    events: RecordedEvent[];
    actions: ActionView[];
    rawEventCount: number;
    referenceImage: ReferenceImage | null;
}
export type RecordingView = DocumentView;
export interface TaskSummary {
    id: string;
    name: string;
    revision: number;
    durationUs: number;
    actionCount: number;
}
export interface Hotkeys {
    recordVk: number;
    stopVk: number;
}
export interface SessionSnapshot {
    mode: "idle" | "recording" | "preparing" | "executing";
    taskId: string;
    revision: number;
    version: number;
    hasRecording: boolean;
    recordingLive: { elapsedUs: number; eventCount: number } | null;
    executionLive: {
        roundsRequested: number;
        currentRound: number;
        eventsInjected: number;
        lastSeq: number;
        stopRequested: boolean;
        prepareRemainingMs: number;
        elapsedUs: number;
        speed: number;
    } | null;
    hasExecutionReport: boolean;
    lastError: string | null;
    hotkeyStatus: string[];
    hotkeys: Hotkeys;
    canUndo: boolean;
    canRedo: boolean;
    isDemo: boolean;
    hasPendingRecording: boolean;
}
export interface WorkspaceView {
    session: SessionSnapshot;
    document: DocumentView;
    tasks: TaskSummary[];
    dataPath: string;
}
export type EditRequest =
    | { op: "insertWait"; afterAction: string | null; durationUs: number }
    | { op: "insertText"; afterAction: string | null; text: string }
    | { op: "updateWait"; actionId: string; durationUs: number }
    | { op: "deleteAction"; actionId: string }
    | { op: "updateMove"; actionId: string; x: number; y: number; durationUs: number; linkNextClick: boolean }
    | { op: "updateClick"; actionId: string; x: number; y: number; button: MouseButton; count: number }
    | { op: "updateWheel"; actionId: string; delta: number }
    | { op: "updateKeys"; actionId: string; chord: string }
    | { op: "updateText"; actionId: string; text: string }
    | { op: "setReference"; reference: ReferenceImage | null };
