import { getCurrentWindow } from "@tauri-apps/api/window";

// 与 Tauri 的 ResizeDirection 字面量一致；该类型未从包里导出。
type ResizeDirection = "East" | "North" | "NorthEast" | "NorthWest" | "South" | "SouthEast" | "SouthWest" | "West";

const HANDLES: { dir: ResizeDirection; className: string }[] = [
    { dir: "North", className: "n" },
    { dir: "South", className: "s" },
    { dir: "West", className: "w" },
    { dir: "East", className: "e" },
    { dir: "NorthWest", className: "nw" },
    { dir: "NorthEast", className: "ne" },
    { dir: "SouthWest", className: "sw" },
    { dir: "SouthEast", className: "se" },
];

/** 无边框窗口失去系统边框后，用窗口边缘的透明手柄找回拖动缩放。 */
export function WindowResizeHandles() {
    return (
        <div className="window-resize-handles" aria-hidden="true">
            {HANDLES.map(handle => (
                <div
                    key={handle.dir}
                    className={`window-resize-handle ${handle.className}`}
                    onPointerDown={event => {
                        if (event.button !== 0) return;
                        event.preventDefault();
                        try {
                            void getCurrentWindow()
                                .startResizeDragging(handle.dir)
                                .catch(() => {});
                        } catch {
                            // 浏览器预览没有窗口对象。
                        }
                    }}
                />
            ))}
        </div>
    );
}
