import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ActionView, EditRequest, MouseButton } from "../types";
export function Modal({
    title,
    children,
    onClose,
    open = true,
    wide = false,
}: {
    title: string;
    children: ReactNode;
    onClose: () => void;
    open?: boolean;
    wide?: boolean;
}) {
    const ref = useRef<HTMLDialogElement>(null);
    useEffect(() => {
        const node = ref.current;
        if (!node) return;
        if (open && !node.open) node.showModal();
        if (!open && node.open) node.close();
        return () => {
            if (node.open) node.close();
        };
    }, [open]);
    return (
        <dialog
            ref={ref}
            className={`dialog${wide ? " dialog-wide" : ""}`}
            aria-label={title}
            onCancel={e => {
                e.preventDefault();
                onClose();
            }}
        >
            <div className="dialog-heading">
                <h2>{title}</h2>
                <button type="button" className="icon-button" aria-label="关闭对话框" onClick={onClose}>
                    ×
                </button>
            </div>
            {children}
        </dialog>
    );
}
export interface EditorTarget {
    action?: ActionView;
    insert?: "wait" | "text";
    after: string | null;
    linkAvailable: boolean;
}
interface Props {
    target: EditorTarget;
    busy: boolean;
    error: string | null;
    picking: boolean;
    pickedPoint: { x: number; y: number } | null;
    onPick: () => void;
    onClose: () => void;
    onSave: (edit: EditRequest) => void;
}
export function EditorDialog(p: Props) {
    const action = p.target.action,
        kind = action?.kind;
    const [x, setX] = useState(kind && "x" in kind ? String(kind.x) : "0"),
        [y, setY] = useState(kind && "y" in kind ? String(kind.y) : "0");
    const [duration, setDuration] = useState(
        String(action ? (action.endUs - action.startUs) / (kind?.type === "wait" ? 1_000_000 : 1000) : 1),
    );
    const [button, setButton] = useState<MouseButton>(kind?.type === "click" ? kind.button : "left");
    const [count, setCount] = useState(kind?.type === "click" ? kind.count : 1);
    const [steps, setSteps] = useState(kind?.type === "wheel" ? String(Math.abs(kind.delta) / 120) : "1");
    const [direction, setDirection] = useState(kind?.type === "wheel" && kind.delta < 0 ? -1 : 1);
    const [inputType, setInputType] = useState<"keys" | "text">(kind?.type === "keys" ? "keys" : "text");
    const [text, setText] = useState(kind?.type === "text" ? kind.text : "");
    const [chord, setChord] = useState(kind?.type === "keys" ? kind.chord : "");
    const [linked, setLinked] = useState(false);
    const [localError, setLocalError] = useState("");
    useEffect(() => {
        if (p.pickedPoint) {
            setX(String(p.pickedPoint.x));
            setY(String(p.pickedPoint.y));
        }
    }, [p.pickedPoint]);
    const type = p.target.insert ?? kind?.type;
    const readOnly = action && !action.editable;
    const coordinateFields = (
        <>
            <div className="field-row">
                <label>
                    终点 X<input type="number" step={1} required value={x} onChange={e => setX(e.target.value)} />
                </label>
                <label>
                    终点 Y<input type="number" step={1} required value={y} onChange={e => setY(e.target.value)} />
                </label>
            </div>
            <button type="button" className="button" onClick={p.onPick}>
                在预览中选点
            </button>
        </>
    );
    return (
        <Modal
            title={
                readOnly
                    ? action.title
                    : p.target.insert
                      ? `插入${type === "wait" ? "等待" : "文本"}`
                      : `编辑${action?.title ?? "动作"}`
            }
            onClose={p.onClose}
            open={!p.picking}
        >
            <form
                className="dialog-form"
                onSubmit={e => {
                    e.preventDefault();
                    setLocalError("");
                    if (readOnly) {
                        p.onClose();
                        return;
                    }
                    const id = action?.id ?? "";
                    let edit: EditRequest;
                    if (type === "wait") {
                        const us = Math.round(Number(duration) * 1_000_000);
                        if (!Number.isSafeInteger(us) || us <= 0) {
                            setLocalError("等待时长必须大于零");
                            return;
                        }
                        edit = p.target.insert
                            ? { op: "insertWait", afterAction: p.target.after, durationUs: us }
                            : { op: "updateWait", actionId: id, durationUs: us };
                    } else if (type === "move") {
                        const px = Number(x),
                            py = Number(y),
                            us = Math.round(Number(duration) * 1000);
                        if (![px, py, us].every(Number.isSafeInteger) || us < 0) {
                            setLocalError("请填写有效的整数坐标和非负耗时");
                            return;
                        }
                        edit = { op: "updateMove", actionId: id, x: px, y: py, durationUs: us, linkNextClick: linked };
                    } else if (type === "click") {
                        const px = Number(x),
                            py = Number(y);
                        if (![px, py].every(Number.isSafeInteger)) {
                            setLocalError("坐标应为整数");
                            return;
                        }
                        edit = { op: "updateClick", actionId: id, x: px, y: py, button, count };
                    } else if (type === "wheel") {
                        const delta = direction * Math.round(Number(steps) * 120);
                        if (!Number.isSafeInteger(delta) || !delta) {
                            setLocalError("滚动量必须大于零");
                            return;
                        }
                        edit = { op: "updateWheel", actionId: id, delta };
                    } else if (p.target.insert === "text")
                        edit = { op: "insertText", afterAction: p.target.after, text };
                    else if (inputType === "keys") edit = { op: "updateKeys", actionId: id, chord };
                    else edit = { op: "updateText", actionId: id, text };
                    p.onSave(edit);
                }}
            >
                {readOnly ? (
                    <div className="dialog-body">
                        <p>{action.detail}</p>
                        <p>
                            {kind?.type === "drag"
                                ? "拖拽保留原始轨迹和按住顺序，暂不提供独立参数编辑。"
                                : "此类输入可保留在录制中，暂不提供参数编辑。"}
                        </p>
                    </div>
                ) : (
                    <>
                        {type === "move" && (
                            <>
                                {coordinateFields}
                                <label>
                                    移动耗时（毫秒）
                                    <input
                                        type="number"
                                        min={0}
                                        step={0.001}
                                        required
                                        value={duration}
                                        onChange={e => setDuration(e.target.value)}
                                    />
                                </label>
                                <label className="checkbox-field">
                                    <input
                                        type="checkbox"
                                        checked={linked}
                                        disabled={!p.target.linkAvailable}
                                        onChange={e => setLinked(e.target.checked)}
                                    />
                                    同时修改紧接着的点击位置
                                </label>
                                <p className="field-help">改变耗时会同步调整后续键鼠动作，保留原有停顿。</p>
                            </>
                        )}
                        {type === "click" && (
                            <>
                                <div className="field-row">
                                    <label>
                                        鼠标按键
                                        <select value={button} onChange={e => setButton(e.target.value as MouseButton)}>
                                            <option value="left">左键</option>
                                            <option value="right">右键</option>
                                            <option value="middle">中键</option>
                                        </select>
                                    </label>
                                    <label>
                                        点击次数
                                        <select value={count} onChange={e => setCount(Number(e.target.value))}>
                                            <option value={1}>单击</option>
                                            <option value={2}>双击</option>
                                        </select>
                                    </label>
                                </div>
                                {coordinateFields}
                            </>
                        )}
                        {type === "wheel" && (
                            <>
                                <label>
                                    方向
                                    <select value={direction} onChange={e => setDirection(Number(e.target.value))}>
                                        <option value={1}>向上</option>
                                        <option value={-1}>向下</option>
                                    </select>
                                </label>
                                <label>
                                    滚动量（格）
                                    <input
                                        type="number"
                                        min={1 / 120}
                                        step="any"
                                        required
                                        value={steps}
                                        onChange={e => setSteps(e.target.value)}
                                    />
                                </label>
                                <p className="field-help">1 格对应 120 个 Windows 原生增量；支持部分格。</p>
                            </>
                        )}
                        {type === "wait" && (
                            <>
                                <label>
                                    等待时长（秒）
                                    <input
                                        type="number"
                                        min={0.000001}
                                        step={0.000001}
                                        required
                                        autoFocus
                                        value={duration}
                                        onChange={e => setDuration(e.target.value)}
                                    />
                                </label>
                                <p className="field-help">后续鼠标、键盘和总时长一起延后，原来的停顿保持不变。</p>
                            </>
                        )}
                        {(type === "keys" || type === "text") && (
                            <>
                                {!p.target.insert && (
                                    <label>
                                        输入方式
                                        <select
                                            value={inputType}
                                            onChange={e => setInputType(e.target.value as "keys" | "text")}
                                        >
                                            <option value="keys">按键或组合键</option>
                                            <option value="text">明确输入文本</option>
                                        </select>
                                    </label>
                                )}
                                {inputType === "keys" && !p.target.insert ? (
                                    <>
                                        <label>
                                            按键或组合键
                                            <input
                                                autoFocus
                                                required
                                                value={chord}
                                                placeholder="例如 Ctrl + S、Tab、A"
                                                onChange={e => setChord(e.target.value)}
                                            />
                                        </label>
                                        <p className="field-help">
                                            按键会保留按下与抬起顺序；录制的按键不会自动识别为文本。
                                        </p>
                                    </>
                                ) : (
                                    <>
                                        <label>
                                            输入文本
                                            <textarea
                                                autoFocus
                                                required
                                                rows={5}
                                                value={text}
                                                onChange={e => setText(e.target.value)}
                                            />
                                        </label>
                                        <p className="field-help">支持中文、符号和换行，直接输入字符，不改动剪贴板。</p>
                                    </>
                                )}
                            </>
                        )}
                    </>
                )}
                {(localError || p.error) && (
                    <p className="form-error" role="alert">
                        {localError || p.error}
                    </p>
                )}
                <div className="dialog-actions">
                    <button type="button" className="button" onClick={p.onClose}>
                        取消
                    </button>
                    <button type="submit" className="button primary" disabled={p.busy}>
                        {readOnly ? "知道了" : p.busy ? "保存中…" : "确认"}
                    </button>
                </div>
            </form>
        </Modal>
    );
}
