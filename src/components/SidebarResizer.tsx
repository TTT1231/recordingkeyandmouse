import { useEffect, useRef, useState, type PointerEvent } from "react";

import { SIDEBAR_DEFAULT, SIDEBAR_MAX, SIDEBAR_MIN, clampSidebarWidth } from "../layout";

interface Props {
    width: number;
    collapsed: boolean;
    onChange: (width: number) => void;
}

/** 任务栏右缘的拖动分隔条：拖动改宽、双击复位，方向键也可调整。 */
export function SidebarResizer({ width, collapsed, onChange }: Props) {
    const drag = useRef<{ startX: number; startWidth: number } | null>(null);
    const [resizing, setResizing] = useState(false);

    const finish = () => {
        if (!drag.current && !resizing) return;
        drag.current = null;
        setResizing(false);
        document.body.classList.remove("is-resizing-sidebar");
    };

    useEffect(() => {
        return () => {
            document.body.classList.remove("is-resizing-sidebar");
        };
    }, []);

    const begin = (event: PointerEvent<HTMLDivElement>) => {
        if (collapsed || event.button !== 0) return;
        event.preventDefault();
        event.currentTarget.focus({ preventScroll: true });
        event.currentTarget.setPointerCapture(event.pointerId);
        drag.current = { startX: event.clientX, startWidth: width };
        setResizing(true);
        document.body.classList.add("is-resizing-sidebar");
    };

    const move = (event: PointerEvent<HTMLDivElement>) => {
        if (!drag.current) return;
        const next = clampSidebarWidth(
            drag.current.startWidth + event.clientX - drag.current.startX,
            window.innerWidth,
        );
        onChange(next);
    };

    const end = (event: PointerEvent<HTMLDivElement>) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId))
            event.currentTarget.releasePointerCapture(event.pointerId);
        finish();
    };

    const resizeBy = (delta: number) => onChange(clampSidebarWidth(width + delta, window.innerWidth));

    return (
        <div
            className={`sidebar-resizer${resizing ? " resizing" : ""}`}
            role="separator"
            aria-orientation="vertical"
            aria-label="调整任务栏宽度"
            aria-valuenow={Math.round(width)}
            aria-valuemin={SIDEBAR_MIN}
            aria-valuemax={SIDEBAR_MAX}
            aria-hidden={collapsed || undefined}
            tabIndex={collapsed ? -1 : 0}
            title="拖动调整任务栏宽度，双击恢复默认"
            onPointerDown={begin}
            onPointerMove={move}
            onPointerUp={end}
            onPointerCancel={end}
            onLostPointerCapture={finish}
            onDoubleClick={() => onChange(SIDEBAR_DEFAULT)}
            onKeyDown={event => {
                if (collapsed) return;
                if (event.key === "ArrowLeft" || event.key === "ArrowRight") {
                    event.preventDefault();
                    resizeBy((event.key === "ArrowRight" ? 1 : -1) * (event.shiftKey ? 32 : 16));
                }
                if (event.key === "Home") {
                    event.preventDefault();
                    onChange(SIDEBAR_MIN);
                }
                if (event.key === "End") {
                    event.preventDefault();
                    onChange(SIDEBAR_MAX);
                }
            }}
        />
    );
}
