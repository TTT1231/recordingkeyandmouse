import { useEffect, useRef, useState, type MouseEvent as ReactMouseEvent, type PointerEvent } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

import type { SessionSnapshot } from "../types";
import { Icon } from "./Icon";

interface Props {
    snapshot: SessionSnapshot;
    busy: boolean;
    actionsOpen: boolean;
    timelineOpen: boolean;
    sidebarOpen: boolean;
    onToggleSidebar: () => void;
    onToggleActions: () => void;
    onToggleTimeline: () => void;
    onCreate: () => void;
    onRename: () => void;
    onUndo: () => void;
    onRedo: () => void;
    onDemo: () => void;
    onShortcuts: () => void;
    onCheckUpdate: () => void;
    onAbout: () => void;
}

async function runWindow(action: (appWindow: ReturnType<typeof getCurrentWindow>) => Promise<void>) {
    try {
        await action(getCurrentWindow());
    } catch {
        // 浏览器预览没有 Tauri 窗口对象；窗口控制保持可见但无动作。
    }
}

/** Windows 自绘窗口按钮：与系统标题栏同样的位置、尺寸和关闭色。 */
export function WindowControls() {
    const [maximized, setMaximized] = useState(false);
    useEffect(() => {
        let active = true;
        let stop: (() => void) | undefined;
        let appWindow: ReturnType<typeof getCurrentWindow>;
        try {
            appWindow = getCurrentWindow();
        } catch {
            return;
        }
        const sync = () => {
            void appWindow
                .isMaximized()
                .then(value => {
                    if (active) setMaximized(value);
                })
                .catch(() => {});
        };
        sync();
        void appWindow
            .onResized(sync)
            .then(unlisten => {
                if (active) stop = unlisten;
                else unlisten();
            })
            .catch(() => {});
        return () => {
            active = false;
            stop?.();
        };
    }, []);
    const toggle = () =>
        void runWindow(async appWindow => {
            await appWindow.toggleMaximize();
            setMaximized(await appWindow.isMaximized());
        });
    return (
        <div className="window-controls" role="group" aria-label="窗口控制">
            <button
                type="button"
                className="window-control"
                aria-label="最小化"
                title="最小化"
                onClick={() => void runWindow(appWindow => appWindow.minimize())}
            >
                <Icon name="window-minimize" size={14} />
            </button>
            <button
                type="button"
                className="window-control"
                aria-label={maximized ? "还原" : "最大化"}
                title={maximized ? "还原" : "最大化"}
                onClick={toggle}
            >
                <Icon name={maximized ? "window-restore" : "window-maximize"} size={14} />
            </button>
            <button
                type="button"
                className="window-control close"
                aria-label="关闭"
                title="关闭"
                onClick={() => void runWindow(appWindow => appWindow.close())}
            >
                <Icon name="window-close" size={14} />
            </button>
        </div>
    );
}

/**
 * 自绘标题栏：左起为任务栏折叠按钮与菜单，右侧是窗口按钮。
 * 空余区域负责拖动移动窗口与双击最大化；拖动在 4px 阈值后才交给原生窗口。
 */
export function TitleBar(p: Props) {
    const disabled = p.busy || p.snapshot.mode !== "idle";
    const drag = useRef<{ x: number; y: number; moved: boolean } | null>(null);
    const beginDrag = (event: PointerEvent<HTMLDivElement>) => {
        if (event.button !== 0) return;
        drag.current = { x: event.clientX, y: event.clientY, moved: false };
    };
    const moveDrag = (event: PointerEvent<HTMLDivElement>) => {
        const state = drag.current;
        if (!state || state.moved) return;
        if (Math.abs(event.clientX - state.x) < 4 && Math.abs(event.clientY - state.y) < 4) return;
        state.moved = true;
        void runWindow(async appWindow => {
            if (await appWindow.isMaximized()) await appWindow.unmaximize();
            await appWindow.startDragging();
        });
    };
    const endDrag = () => {
        drag.current = null;
    };
    const menubar = useRef<HTMLElement>(null);
    useEffect(() => {
        const closeAll = () => {
            menubar.current?.querySelectorAll<HTMLDetailsElement>("details.app-menu[open]").forEach(node => {
                node.open = false;
            });
        };
        const onKey = (event: KeyboardEvent) => {
            if (event.key !== "Escape") return;
            const open = menubar.current?.querySelector<HTMLDetailsElement>("details.app-menu[open]");
            if (!open) return;
            event.preventDefault();
            open.open = false;
            open.querySelector<HTMLElement>("summary")?.focus();
        };
        window.addEventListener("blur", closeAll);
        window.addEventListener("keydown", onKey);
        return () => {
            window.removeEventListener("blur", closeAll);
            window.removeEventListener("keydown", onKey);
        };
    }, []);
    // 原生 details 默认允许多个菜单同时展开；打开新菜单时关掉其余菜单。
    const closeOtherMenus = (event: ReactMouseEvent<HTMLElement>) => {
        const summary = event.target instanceof Element ? event.target.closest("summary") : null;
        if (!summary) return;
        menubar.current?.querySelectorAll<HTMLDetailsElement>("details.app-menu[open]").forEach(node => {
            if (node !== summary.parentElement) node.open = false;
        });
    };
    return (
        <header className="app-titlebar">
            <div
                className="titlebar-drag"
                aria-hidden="true"
                onPointerDown={beginDrag}
                onPointerMove={moveDrag}
                onPointerUp={endDrag}
                onPointerCancel={endDrag}
                onDoubleClick={() => void runWindow(appWindow => appWindow.toggleMaximize())}
            />
            <button
                type="button"
                className="titlebar-toggle"
                aria-label={p.sidebarOpen ? "收起任务栏" : "展开任务栏"}
                aria-controls="task-sidebar"
                aria-expanded={p.sidebarOpen}
                title={p.sidebarOpen ? "收起任务栏" : "展开任务栏"}
                onClick={p.onToggleSidebar}
            >
                <Icon name="sidebar" size={18} />
            </button>
            <div className="titlebar-history" role="group" aria-label="历史记录">
                <button
                    type="button"
                    className="titlebar-action"
                    aria-label="撤销"
                    title="撤销（Ctrl+Z）"
                    disabled={disabled || !p.snapshot.canUndo}
                    onClick={p.onUndo}
                >
                    <Icon name="undo" size={17} />
                </button>
                <button
                    type="button"
                    className="titlebar-action"
                    aria-label="重做"
                    title="重做（Ctrl+Y）"
                    disabled={disabled || !p.snapshot.canRedo}
                    onClick={p.onRedo}
                >
                    <Icon name="redo" size={17} />
                </button>
            </div>
            <nav ref={menubar} className="app-menubar" aria-label="应用菜单" onClick={closeOtherMenus}>
                <details className="app-menu">
                    <summary>文件</summary>
                    <div className="menu-popup">
                        <button type="button" disabled={disabled || p.snapshot.isDemo} onClick={p.onCreate}>
                            新建任务 <kbd>Ctrl N</kbd>
                        </button>
                        <button type="button" disabled={disabled || p.snapshot.isDemo} onClick={p.onRename}>
                            重命名任务
                        </button>
                    </div>
                </details>
                <details className="app-menu">
                    <summary>视图</summary>
                    <div className="menu-popup">
                        <button type="button" aria-expanded={p.actionsOpen} onClick={p.onToggleActions}>
                            录制内容
                        </button>
                        <button type="button" aria-expanded={p.timelineOpen} onClick={p.onToggleTimeline}>
                            时间线
                        </button>
                    </div>
                </details>
                <details className="app-menu">
                    <summary>帮助</summary>
                    <div className="menu-popup">
                        <button type="button" disabled={disabled || p.snapshot.isDemo} onClick={p.onDemo}>
                            使用演示
                        </button>
                        <div className="menu-separator" />
                        <button type="button" disabled={disabled} onClick={p.onShortcuts}>
                            快捷键设置…
                        </button>
                        <button type="button" onClick={p.onCheckUpdate}>
                            检查更新…
                        </button>
                        <button type="button" onClick={p.onAbout}>
                            关于 Repeat
                        </button>
                    </div>
                </details>
            </nav>
            <WindowControls />
        </header>
    );
}
