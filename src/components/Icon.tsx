// 少量内联 SVG 图标（对齐原型 data-icon 的视觉语言）。

const PATHS: Record<string, string> = {
    plus: "M8 3v5H3v2h5v5h2v-5h5V8h-5V3H8Z",
    play: "M6 4l10 6-10 6V4Z",
    pause: "M6 4h3v12H6zM11 4h3v12h-3z",
    previous: "M7 4v12L15 4v12L7 4ZM5 4h1.5v12H5z",
    next: "M11 4l8 6-8 6V4ZM3.5 4H5v12H3.5z",
    record: "M10 3a7 7 0 100 14 7 7 0 000-14Z",
    "record-ring": "M10 2a8 8 0 100 16 8 8 0 000-16zm0 2.5a5.5 5.5 0 110 11 5.5 5.5 0 010-11z",
    mouse: "M12 2a7 7 0 017 7v6a7 7 0 01-14 0V9a7 7 0 017-7zm0 2a5 5 0 00-5 5v6a5 5 0 0010 0V9a5 5 0 00-5-5z",
    keyboard:
        "M3 5h18v12H3V5zm2 2v2h2V7H5zm3 0v2h2V7H8zm3 0v2h2V7h-2zm3 0v2h2V7h-2zm3 0v2h2V7h-2zM5 10v2h2v-2H5zm3 0v2h2v-2H8zm3 0v2h2v-2h-2zm3 0v2h2v-2h-2zm3 0v2h2v-2h-2zM7 13v2h10v-2H7z",
    clock: "M12 3a9 9 0 100 18 9 9 0 000-18zm0 2a7 7 0 110 14 7 7 0 010-14zm-1 2v6l5 2 .7-1.9-3.7-1.5V7h-2z",
    cursor: "M5 3l14 8-6 1.5L10 19 5 3z",
    layers: "M12 3l9 5-9 5-9-5 9-5zm0 8.2l6.3-3.5L21 9.5 12 14.5 3 9.5l2.7-1.8L12 11.2zm0 4l6.3-3.5L21 13.5 12 18.5l-9-5 2.7-1.8L12 15.2z",
    x: "M5.7 4.3L12 10.6l6.3-6.3 1.4 1.4L13.4 12l6.3 6.3-1.4 1.4L12 13.4l-6.3 6.3-1.4-1.4L10.6 12 4.3 5.7l1.4-1.4z",
    more: "M5 12a2 2 0 11-4 0 2 2 0 014 0zm9 0a2 2 0 11-4 0 2 2 0 014 0zm7 2a2 2 0 100-4 2 2 0 000 4z",
    minus: "M4 9h12v2H4z",
    maximize: "M4 4h12v8H4V4zm2 2v4h8V6H6z",
    image: "M3 5h14v10H3V5zm2 2v4.5l2.5-2.5 2 2L12 8l3 3.5V7H5zm7 1a1.2 1.2 0 100 2.4A1.2 1.2 0 0012 8z",
    arrowLeft: "M10 4l-7 6 7 6v-4h7v-4h-7V4z",
    folder: "M3 5h5l2 2h7v9H3V5z",
    check: "M15.5 5.5l2 2L9 16l-5-5 2-2 3 3 6.5-6.5z",
    repeat: "M14.6 2.6l4 3.9-4 3.9-1.4-1.4 1.6-1.5H8.5A3.5 3.5 0 005 11v1.5H3V11a5.5 5.5 0 015.5-5.5h6.3l-1.6-1.5 1.4-1.4zM5.4 17.4l4-3.9 1.4 1.4-1.6 1.5h6.3a3.5 3.5 0 003.5-3.5V11.5h2V13a5.5 5.5 0 01-5.5 5.5H9.2l1.6 1.5-1.4 1.4-4-3.9z",
};

export function Icon({ name, size = 14 }: { name: string; size?: number }) {
    const d = PATHS[name] ?? PATHS.record;
    return (
        <svg
            width={size}
            height={size}
            viewBox="0 0 20 20"
            fill="currentColor"
            aria-hidden="true"
            style={{ flex: "none" }}
        >
            <path d={d} fillRule={name === "record-ring" ? "evenodd" : undefined} />
        </svg>
    );
}
