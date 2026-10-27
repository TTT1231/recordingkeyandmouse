import { useEffect, useRef, useState } from "react";

export const TIMELINE_LANE_DEFAULT = 44;
export const TIMELINE_LANE_MIN = 44;
export const TIMELINE_LANE_MAX = 100;
export const TIMELINE_HEIGHT_KEY = "repeat.timeline.laneHeight";

// 面板还包含刻度、轨道间隔和上下留白；尽量给上方工作区留出 450px。
const TIMELINE_FIXED_HEIGHT = 86;
const WORKSPACE_MIN_HEIGHT = 450;

export function maxTimelineLane(viewportHeight: number) {
    if (!Number.isFinite(viewportHeight)) return TIMELINE_LANE_MAX;
    return Math.max(
        TIMELINE_LANE_MIN,
        Math.min(TIMELINE_LANE_MAX, Math.floor((viewportHeight - WORKSPACE_MIN_HEIGHT - TIMELINE_FIXED_HEIGHT) / 2)),
    );
}

export function clampTimelineLane(value: number, viewportHeight = window.innerHeight) {
    if (!Number.isFinite(value)) return TIMELINE_LANE_DEFAULT;
    return Math.max(TIMELINE_LANE_MIN, Math.min(maxTimelineLane(viewportHeight), Math.round(value)));
}

export function TimelineResizer({ laneHeight, onChange }: { laneHeight: number; onChange: (value: number) => void }) {
    const drag = useRef<{ y: number; height: number } | null>(null);
    const [resizing, setResizing] = useState(false);
    const [viewportHeight, setViewportHeight] = useState(() => window.innerHeight);
    useEffect(() => {
        const update = () => setViewportHeight(window.innerHeight);
        window.addEventListener("resize", update);
        return () => window.removeEventListener("resize", update);
    }, []);
    useEffect(() => () => document.body.classList.remove("is-resizing-timeline"), []);
    const end = () => {
        drag.current = null;
        setResizing(false);
        document.body.classList.remove("is-resizing-timeline");
    };
    return (
        <div
            className={`timeline-resizer${resizing ? " resizing" : ""}`}
            role="separator"
            aria-label="调整时间线高度"
            aria-orientation="horizontal"
            aria-valuemin={TIMELINE_LANE_MIN}
            aria-valuemax={maxTimelineLane(viewportHeight)}
            aria-valuenow={laneHeight}
            tabIndex={0}
            title="拖动调整时间线高度，双击恢复默认"
            onPointerDown={event => {
                if (event.button !== 0) return;
                event.currentTarget.setPointerCapture(event.pointerId);
                drag.current = { y: event.clientY, height: laneHeight };
                setResizing(true);
                document.body.classList.add("is-resizing-timeline");
            }}
            onPointerMove={event => {
                if (drag.current)
                    onChange(
                        clampTimelineLane(drag.current.height + (drag.current.y - event.clientY) / 2, viewportHeight),
                    );
            }}
            onPointerUp={event => {
                end();
                if (event.currentTarget.hasPointerCapture(event.pointerId))
                    event.currentTarget.releasePointerCapture(event.pointerId);
            }}
            onPointerCancel={end}
            onDoubleClick={() => onChange(clampTimelineLane(TIMELINE_LANE_DEFAULT, viewportHeight))}
            onKeyDown={event => {
                if (event.key === "ArrowUp" || event.key === "ArrowDown") {
                    event.preventDefault();
                    onChange(clampTimelineLane(laneHeight + (event.key === "ArrowUp" ? 4 : -4), viewportHeight));
                } else if (event.key === "Home") {
                    event.preventDefault();
                    onChange(clampTimelineLane(TIMELINE_LANE_DEFAULT, viewportHeight));
                } else if (event.key === "End") {
                    event.preventDefault();
                    onChange(maxTimelineLane(viewportHeight));
                }
            }}
        />
    );
}
