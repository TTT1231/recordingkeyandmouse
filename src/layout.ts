// 任务侧栏的折叠与宽度：纯函数，便于单测；组件只保存状态。
export const SIDEBAR_MIN = 184;
export const SIDEBAR_MAX = 420;
export const SIDEBAR_DEFAULT = 240;
export const SIDEBAR_WIDTH_KEY = "repeat.sidebar.width";
export const SIDEBAR_OPEN_KEY = "repeat.sidebar.open";

/** 右侧预览至少留 560px（录制内容 300 + 间隔与预览），避免把工作区挤没。 */
export function clampSidebarWidth(width: number, viewportWidth = Number.POSITIVE_INFINITY): number {
    if (!Number.isFinite(width)) return SIDEBAR_DEFAULT;
    const viewportLimit = Number.isFinite(viewportWidth)
        ? Math.max(SIDEBAR_MIN, Math.floor(viewportWidth - 560))
        : SIDEBAR_MAX;
    return Math.max(SIDEBAR_MIN, Math.min(SIDEBAR_MAX, viewportLimit, Math.round(width)));
}
