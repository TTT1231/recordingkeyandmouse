import type { ActionView } from "./types";
export type DisplayRow = ActionView;
/** 当前播放动作与手动选择分别表达；停顿期间不把上个动作标成仍在执行。 */
export function actionAtTime(actions: ActionView[], timeUs: number): ActionView | null {
    let active: ActionView | null = null;
    for (const action of actions) {
        if (action.startUs > timeUs) break;
        if (timeUs <= action.endUs || (action.endUs === action.startUs && timeUs - action.startUs < 100_000))
            active = action;
    }
    return active;
}
export function desktopBounds(monitors: { x: number; y: number; width: number; height: number }[]) {
    if (!monitors.length) return { x: 0, y: 0, width: 1920, height: 1080 };
    const x = Math.min(...monitors.map(m => m.x));
    const y = Math.min(...monitors.map(m => m.y));
    return {
        x,
        y,
        width: Math.max(...monitors.map(m => m.x + m.width)) - x,
        height: Math.max(...monitors.map(m => m.y + m.height)) - y,
    };
}
