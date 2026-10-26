// 内联 SVG 图标：视觉语言对齐 docs/index.html 原型（24 网格、1.75 描边、圆角端点）。
// 同一套图标同时用于工具条、时间线与录制内容，避免同类操作出现粗细/风格不一致。

import type { ReactNode } from "react";

const ICONS: Record<string, ReactNode> = {
    plus: <path d="M12 5v14M5 12h14" />,
    minus: <path d="M5 12h14" />,
    x: <path d="m6 6 12 12M6 18 18 6" />,
    sidebar: (
        <>
            <rect x="3" y="4" width="18" height="16" rx="3" />
            <path d="M9 4v16" />
        </>
    ),
    "window-minimize": <path d="M4 12h16" />,
    "window-maximize": <rect x="4" y="4" width="16" height="16" rx="1.5" />,
    "window-restore": (
        <>
            <rect x="4" y="8" width="12" height="12" rx="1.5" />
            <path d="M8 8V4h12v12h-4" />
        </>
    ),
    "window-close": <path d="M5 5l14 14M19 5 5 19" />,
    undo: <path d="M3 10h11a7 7 0 0 1 0 14M3 10l6-6m-6 6 6 6" transform="translate(0 -3)" />,
    redo: <path d="M21 10H10a7 7 0 0 0 0 14m11-14-6-6m6 6-6 6" transform="translate(0 -3)" />,
    more: (
        <>
            <circle cx="5" cy="12" r="1" fill="currentColor" stroke="none" />
            <circle cx="12" cy="12" r="1" fill="currentColor" stroke="none" />
            <circle cx="19" cy="12" r="1" fill="currentColor" stroke="none" />
        </>
    ),
    play: <path d="m8 4 12 8-12 8V4Z" />,
    pause: <path d="M8 5v14M16 5v14" />,
    previous: <path d="M5 5v14m14-14-10 7 10 7V5Z" />,
    next: <path d="M19 5v14M5 5l10 7-10 7V5Z" />,
    record: (
        <>
            <circle cx="12" cy="12" r="9" />
            <circle cx="12" cy="12" r="4" />
        </>
    ),
    "record-ring": <circle cx="12" cy="12" r="9" />,
    check: (
        <>
            <circle cx="12" cy="12" r="9" />
            <path d="m8 12 3 3 5-6" />
        </>
    ),
    mouse: (
        <>
            <rect x="6" y="2" width="12" height="20" rx="6" />
            <path d="M12 2v7M6 10h12" />
        </>
    ),
    "mouse-left": (
        <>
            <path d="M5 10V9a7 7 0 0 1 7-7v8Z" fill="currentColor" stroke="none" />
            <rect x="5" y="2" width="14" height="20" rx="7" />
            <path d="M12 2v8M5 10h14" />
        </>
    ),
    "mouse-right": (
        <>
            <path d="M12 2a7 7 0 0 1 7 7v1h-7Z" fill="currentColor" stroke="none" />
            <rect x="5" y="2" width="14" height="20" rx="7" />
            <path d="M12 2v8M5 10h14" />
        </>
    ),
    "mouse-middle": (
        <>
            <rect x="5" y="2" width="14" height="20" rx="7" />
            <path d="M5 10h4m6 0h4" />
            <rect x="10" y="5" width="4" height="7" rx="2" fill="currentColor" stroke="none" />
        </>
    ),
    "wheel-up": (
        <>
            <rect x="5" y="2" width="14" height="20" rx="7" />
            <path d="m9 11 3-3 3 3m-3-3v9" />
        </>
    ),
    "wheel-down": (
        <>
            <rect x="5" y="2" width="14" height="20" rx="7" />
            <path d="m9 13 3 3 3-3m-3 3V7" />
        </>
    ),
    scroll: (
        <>
            <rect x="6" y="2" width="12" height="20" rx="6" />
            <path d="m9 10 3-3 3 3m-6 4 3 3 3-3" />
        </>
    ),
    keyboard: (
        <>
            <rect x="2" y="5" width="20" height="14" rx="3" />
            <path d="M6 9h.01M10 9h.01M14 9h.01M18 9h.01M6 12h.01M10 12h.01M14 12h.01M18 12h.01M7 15h10" />
        </>
    ),
    move: <path d="M12 3v18M3 12h18m-12-6 3-3 3 3m-6 12 3 3 3-3M6 9l-3 3 3 3m12-6 3 3-3 3" />,
    cursor: (
        <>
            <path d="m4 3 7.5 18 2.5-7 7-2.5L4 3Z" />
            <path d="m14 14 6 6" />
        </>
    ),
    clock: (
        <>
            <circle cx="12" cy="12" r="9" />
            <path d="M12 7v5l3 2" />
        </>
    ),
    layers: <path d="m12 3 9 5-9 5-9-5 9-5ZM3 12l9 5 9-5M3 16l9 5 9-5" />,
    repeat: (
        <>
            <path d="m17 2 4 4-4 4" />
            <path d="M3 11V9a3 3 0 0 1 3-3h15" />
            <path d="M7 22l-4-4 4-4" />
            <path d="M21 13v2a3 3 0 0 1-3 3H3" />
        </>
    ),
    folder: <path d="M3 7V5a2 2 0 0 1 2-2h5l2 3h7a2 2 0 0 1 2 2v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7Z" />,
    maximize: <path d="M8 3H3v5m13-5h5v5M3 16v5h5m13-5v5h-5" />,
    image: (
        <>
            <rect x="3" y="3" width="18" height="18" rx="3" />
            <circle cx="8" cy="8" r="1.5" />
            <path d="m3 17 5-5 4 4 4-6 5 7" />
        </>
    ),
    arrowLeft: <path d="M20 12H4m6-6-6 6 6 6" />,
    text: <path d="M4 5V3h16v2M12 3v18M8 21h8" />,
    enter: <path d="M20 4v8H4m5-5-5 5 5 5" />,
};

export function Icon({ name, size = 16 }: { name: string; size?: number }) {
    const glyph = ICONS[name] ?? ICONS.record;
    return (
        <svg
            width={size}
            height={size}
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={1.75}
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
            style={{ flex: "none" }}
        >
            {glyph}
        </svg>
    );
}
