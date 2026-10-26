/** “重复方式”是运行设置，不是录制内容；只影响调度轮数，不改事件。 */
export type RepeatMode = "infinite" | "once" | "count";
/** Rust 用 u32::MAX 表示无限循环；调度器靠停止信号退出。 */
export const INFINITE_ROUNDS = 4_294_967_295;
export const MAX_REPEAT_COUNT = 100_000;
export function parseRepeatCount(value: string): number | null {
    const count = Number(value);
    return Number.isInteger(count) && count >= 1 && count <= MAX_REPEAT_COUNT ? count : null;
}
export function repeatRounds(mode: RepeatMode, count: string): number | null {
    if (mode === "infinite") return INFINITE_ROUNDS;
    if (mode === "once") return 1;
    return parseRepeatCount(count);
}
export function repeatModeLabel(mode: RepeatMode, count: string): string {
    if (mode === "infinite") return "无限循环";
    if (mode === "once") return "只执行一次";
    return `执行 ${count || "—"} 次`;
}
export function repeatSummary(mode: RepeatMode, count: string): string {
    if (mode === "infinite") return "";
    if (mode === "once") return "1";
    return count || "—";
}
