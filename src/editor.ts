// 编辑窗口的请求构造：把表单值转换成后端 EditRequest；只做格式校验，不猜业务含义。
import type { ActionView, EditRequest, MouseButton } from "./types";

export interface EditorTarget {
    action?: ActionView;
    insert?: "wait" | "text";
    after: string | null;
    linkAvailable: boolean;
}
export interface EditorFormValues {
    x: string;
    y: string;
    /** 等待与移动耗时都按秒填写，与原型一致。 */
    duration: string;
    button: MouseButton;
    count: number;
    steps: string;
    direction: number;
    text: string;
    chord: string;
    linked: boolean;
}
export type EditorBuildResult = { edit: EditRequest } | { error: string };

export function buildEditRequest(target: EditorTarget, values: EditorFormValues): EditorBuildResult {
    if (target.action && !target.action.editable) return { error: "此动作暂不支持参数编辑" };
    const type = target.insert ?? target.action?.kind.type;
    const id = target.action?.id ?? "";
    if (type === "wait") {
        const us = Math.round(Number(values.duration) * 1_000_000);
        if (!Number.isSafeInteger(us) || us <= 0) return { error: "等待时长必须大于零" };
        return {
            edit: target.insert
                ? { op: "insertWait", afterAction: target.after, durationUs: us }
                : { op: "updateWait", actionId: id, durationUs: us },
        };
    }
    if (type === "move") {
        const px = Number(values.x),
            py = Number(values.y),
            us = Math.round(Number(values.duration) * 1_000_000);
        if (![px, py, us].every(Number.isSafeInteger) || us < 0) return { error: "请填写有效的整数坐标和非负耗时" };
        return { edit: { op: "updateMove", actionId: id, x: px, y: py, durationUs: us, linkNextClick: values.linked } };
    }
    if (type === "click") {
        const px = Number(values.x),
            py = Number(values.y);
        if (![px, py].every(Number.isSafeInteger)) return { error: "坐标应为整数" };
        return { edit: { op: "updateClick", actionId: id, x: px, y: py, button: values.button, count: values.count } };
    }
    if (type === "wheel") {
        const delta = values.direction * Math.round(Number(values.steps) * 120);
        if (!Number.isSafeInteger(delta) || !delta) return { error: "滚动量必须大于零" };
        return { edit: { op: "updateWheel", actionId: id, delta } };
    }
    if (target.insert === "text") return { edit: { op: "insertText", afterAction: target.after, text: values.text } };
    if (type === "keys") return { edit: { op: "updateKeys", actionId: id, chord: values.chord } };
    if (type === "text") return { edit: { op: "updateText", actionId: id, text: values.text } };
    return { error: "此动作暂不支持参数编辑" };
}
