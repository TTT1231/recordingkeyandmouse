import { useEffect, useRef, useState } from "react";
import * as api from "./api";
import { formatTimelineTick, stopKeyNames, vkName } from "./format";
import { INFINITE_ROUNDS } from "./repeat";
import type { SessionSnapshot } from "./types";
import "./ReplayOverlay.css";

/** The window is click-through. The native mouse hook handles physical clicks on the stop area. */
export function ReplayOverlay() {
    const [snapshot, setSnapshot] = useState<SessionSnapshot | null>(null);
    const version = useRef(0);
    useEffect(() => {
        let active = true;
        const accept = (next: SessionSnapshot) => {
            if (!active || next.version < version.current) return;
            version.current = next.version;
            setSnapshot(next);
        };
        const refresh = () =>
            void api
                .fetchStatus()
                .then(accept)
                .catch(() => {});
        const subscription = api.onStateChanged(accept).catch(() => () => {});
        refresh();
        const timer = window.setInterval(refresh, 250);
        return () => {
            active = false;
            window.clearInterval(timer);
            void subscription.then(unlisten => unlisten());
        };
    }, []);

    const live = snapshot?.executionLive;
    const recording = snapshot?.mode === "recording";
    const preparingRecording = snapshot?.mode === "preparingRecording";
    const recordingMode = recording || preparingRecording;
    const recordHotkeyAvailable = snapshot?.recordHotkeyAvailable === true;
    const preparing = snapshot?.mode === "preparing" || preparingRecording;
    const executing = snapshot?.mode === "executing";
    const stopping = live?.stopRequested ?? false;
    const prepareRemainingMs = preparingRecording
        ? snapshot?.recordingLive?.prepareRemainingMs
        : live?.prepareRemainingMs;
    const stopAction = preparing ? "取消" : "停止";
    const recordKey = vkName(snapshot?.hotkeys.recordVk ?? 0x77);
    const stopKeys = preparingRecording
        ? recordHotkeyAvailable
            ? `${recordKey} / Esc`
            : "Esc"
        : recording
          ? recordHotkeyAvailable
              ? recordKey
              : null
          : stopKeyNames(snapshot?.hotkeys.stopVk ?? 0x78, snapshot?.stopHotkeyAvailable === true);
    const round =
        live?.roundsRequested === INFINITE_ROUNDS
            ? `第 ${live.currentRound} 轮`
            : `第 ${live?.currentRound ?? 0} / ${live?.roundsRequested ?? 1} 轮`;
    return (
        <div
            className={`replay-overlay${recording ? " replay-overlay-recording" : ""}${preparingRecording ? " replay-overlay-preparing-recording" : ""}`}
            role="status"
            aria-label={recordingMode ? "录制状态" : "系统执行状态"}
            aria-atomic="true"
        >
            <span className="replay-overlay-dot" aria-hidden="true" />
            <span className="replay-overlay-details">
                <span className="replay-overlay-line">
                    <span className="replay-overlay-label">
                        {preparingRecording
                            ? "准备录制"
                            : recording
                              ? "录制中"
                              : stopping
                                ? "正在停止"
                                : preparing
                                  ? "准备执行"
                                  : executing
                                    ? "执行中"
                                    : "准备中"}
                    </span>
                    {(preparing || executing) && (
                        <span className="replay-overlay-round">
                            {preparing ? `${Math.max(1, Math.ceil((prepareRemainingMs ?? 0) / 1000))} 秒后开始` : round}
                        </span>
                    )}
                </span>
                <span className="replay-overlay-line replay-overlay-secondary">
                    <span>
                        {stopKeys ? `${stopKeys} ${stopAction}` : `点击${stopAction}`}
                        {!recordingMode && live?.speed && live.speed !== 1 ? ` · ${live.speed}x` : ""}
                    </span>
                    <span className="replay-overlay-time">
                        {formatTimelineTick(
                            recordingMode ? (snapshot?.recordingLive?.elapsedUs ?? 0) : (live?.elapsedUs ?? 0),
                        )}
                    </span>
                </span>
            </span>
            {/* This exact box is also registered as the physical stop target by the native window. */}
            <span
                className="replay-overlay-stop"
                aria-label={`${stopAction}${recordingMode ? "录制" : "执行"}${stopKeys ? `，或按 ${stopKeys}` : ""}`}
            >
                <span />
            </span>
        </div>
    );
}
