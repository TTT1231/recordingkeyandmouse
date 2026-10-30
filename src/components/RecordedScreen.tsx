import { convertFileSrc } from "@tauri-apps/api/core";
import { useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { screenTimeAt } from "../screenRecording";
import type { ScreenRecording } from "../types";

interface Props {
    recording: ScreenRecording;
    timeUs: number;
    playing: boolean;
    speed: number;
    style: CSSProperties;
}

/** This media element is a local historical view. It never sends system input. */
export function RecordedScreen(p: Props) {
    const videoRef = useRef<HTMLVideoElement>(null);
    const playPending = useRef(false);
    const lastSync = useRef({ spanIndex: -1, playing: false, frameSeekAt: -Infinity, playheadUs: 0, syncedAt: 0 });
    const pendingSeek = useRef<number | null>(null);
    const [status, setStatus] = useState<"loading" | "ready" | "error">("loading");
    const [attempt, setAttempt] = useState(0);
    const source = useMemo(() => convertFileSrc(`${p.recording.id}.mp4`, "recording"), [p.recording.id]);
    const sync = () => {
        const video = videoRef.current;
        if (!video || video.readyState < 1 || video.error || status === "error") return;
        const mapped = screenTimeAt(p.recording, p.timeUs);
        const duration = Number.isFinite(video.duration) ? video.duration : p.recording.durationUs / 1_000_000;
        const seconds = Math.min(mapped.sourceTimeUs / 1_000_000, Math.max(0, duration - 0.000001));
        const requestedRate = mapped.rate * p.speed;
        let continuous = p.playing && requestedRate > 0 && mapped.sourceTimeUs < p.recording.durationUs;
        if (continuous && video.playbackRate !== requestedRate) {
            try {
                video.playbackRate = requestedRate;
            } catch {
                // Large time edits can exceed the browser's playback-rate range.
                // Seek bounded frames instead of changing the user's timeline speed.
                continuous = false;
            }
        }
        if (!continuous) video.pause();
        const now = performance.now();
        const previous = lastSync.current;
        const boundary = mapped.spanIndex !== lastSync.current.spanIndex || continuous !== lastSync.current.playing;
        const drift = Math.abs(video.currentTime - seconds);
        const expectedStep = p.playing && previous.playing ? (now - previous.syncedAt) * 1000 * p.speed : 0;
        const jumped = previous.syncedAt > 0 && Math.abs(p.timeUs - previous.playheadUs - expectedStep) > 80_000;
        const frameDue = !p.playing || requestedRate === 0 || now - lastSync.current.frameSeekAt >= 50;
        // Let normal playback decode continuously. Repeated seeks while the
        // decoder is buffering discard useful frames and can prevent recovery.
        const correctDrift = continuous && video.readyState >= 3 && now - previous.frameSeekAt >= 1000 && drift > 0.35;
        if ((boundary || jumped || (!continuous && frameDue) || correctDrift) && drift > 0.001)
            pendingSeek.current = seconds;
        if (!video.seeking && pendingSeek.current !== null) {
            if (Math.abs(video.currentTime - pendingSeek.current) > 0.001) video.currentTime = pendingSeek.current;
            pendingSeek.current = null;
            lastSync.current.frameSeekAt = now;
        }
        lastSync.current.spanIndex = mapped.spanIndex;
        lastSync.current.playing = continuous;
        lastSync.current.playheadUs = p.timeUs;
        lastSync.current.syncedAt = now;
        if (continuous && video.paused && !playPending.current) {
            playPending.current = true;
            void video
                .play()
                .catch((error: unknown) => {
                    if (videoRef.current !== video) return;
                    // A new seek or pause can cancel a pending play request.
                    if (!(error instanceof DOMException && error.name === "AbortError")) setStatus("error");
                })
                .finally(() => {
                    if (videoRef.current === video) playPending.current = false;
                });
        }
    };
    useEffect(sync, [p.timeUs, p.playing, p.speed, p.recording, status, attempt]);
    useEffect(() => {
        const video = videoRef.current;
        return () => video?.pause();
    }, [attempt]);

    return (
        <>
            <video
                key={attempt}
                ref={videoRef}
                className="recorded-screen"
                style={p.style}
                src={source}
                aria-label="录制时的桌面画面"
                muted
                playsInline
                preload="auto"
                disablePictureInPicture
                onLoadedMetadata={sync}
                onLoadedData={() => {
                    setStatus("ready");
                    sync();
                }}
                onSeeked={sync}
                onError={() => setStatus("error")}
            />
            {status !== "ready" && (
                <div className="recorded-screen-message" role="status">
                    {status === "loading" ? (
                        <span>正在读取录制画面…</span>
                    ) : (
                        <>
                            <strong>录制画面无法读取</strong>
                            <span>请确认本地录制文件仍在，且可以正常打开。</span>
                            <button
                                type="button"
                                className="button"
                                onClick={() => {
                                    playPending.current = false;
                                    lastSync.current = {
                                        spanIndex: -1,
                                        playing: false,
                                        frameSeekAt: -Infinity,
                                        playheadUs: 0,
                                        syncedAt: 0,
                                    };
                                    pendingSeek.current = null;
                                    setStatus("loading");
                                    setAttempt(value => value + 1);
                                }}
                            >
                                重新读取
                            </button>
                        </>
                    )}
                </div>
            )}
        </>
    );
}
