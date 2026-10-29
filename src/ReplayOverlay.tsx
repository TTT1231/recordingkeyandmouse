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
    const recordHotkeyAvailable = snapshot?.recordHotkeyAvailable === true;
    const preparing = snapshot?.mode === "preparing";
    const executing = snapshot?.mode === "executing";
    const stopping = live?.stopRequested ?? false;
    const round =
        live?.roundsRequested === INFINITE_ROUNDS
            ? `第 ${live.currentRound} 轮`
            : `第 ${live?.currentRound ?? 0} / ${live?.roundsRequested ?? 1} 轮`;
    return (
        <div
            className={`replay-overlay${recording ? " replay-overlay-recording" : ""}`}
            role="status"
            aria-label={recording ? "录制状态" : "系统执行状态"}
        >
            <span className="replay-overlay-dot" aria-hidden="true" />
            <span className="replay-overlay-details">
                <span className="replay-overlay-line">
                    <span className={`replay-overlay-label${executing && !stopping ? " replay-overlay-active" : ""}`}>
                        {recording
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
                            {preparing
                                ? `${Math.max(1, Math.ceil((live?.prepareRemainingMs ?? 0) / 1000))} 秒后开始`
                                : round}
                        </span>
                    )}
                </span>
                <span className="replay-overlay-line replay-overlay-secondary">
                    <span>
                        {recording
                            ? recordHotkeyAvailable
                                ? `${vkName(snapshot?.hotkeys.recordVk ?? 0x77)} 停止`
                                : "点击停止"
                            : `${stopKeyNames(snapshot?.hotkeys.stopVk ?? 0x78, snapshot?.stopHotkeyAvailable === true)} 停止`}
                        {!recording && live?.speed && live.speed !== 1 ? ` · ${live.speed}x` : ""}
                    </span>
                    <span className="replay-overlay-time">
                        {formatTimelineTick(
                            recording ? (snapshot?.recordingLive?.elapsedUs ?? 0) : (live?.elapsedUs ?? 0),
                        )}
                    </span>
                </span>
            </span>
            {/* This exact box is also registered as the physical stop target by the native window. */}
            <span
                className="replay-overlay-stop"
                aria-label={
                    recording && recordHotkeyAvailable
                        ? `停止录制，或按 ${vkName(snapshot?.hotkeys.recordVk ?? 0x77)}`
                        : recording
                          ? "停止录制"
                          : `停止执行，或按 ${stopKeyNames(snapshot?.hotkeys.stopVk ?? 0x78, snapshot?.stopHotkeyAvailable === true)}`
                }
            >
                <span />
            </span>
        </div>
    );
}
