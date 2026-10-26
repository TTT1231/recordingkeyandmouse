import { useEffect, useRef, useState, type ReactNode } from "react";
import { buildEditRequest, type EditorTarget } from "../editor";
import type { EditRequest, MouseButton } from "../types";
import { Icon } from "./Icon";
export type { EditorTarget } from "../editor";
export function Modal({
    title,
    children,
    onClose,
    open = true,
    wide = false,
    icon,
}: {
    title: string;
    children: ReactNode;
    onClose: () => void;
    open?: boolean;
    wide?: boolean;
    icon?: ReactNode;
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
                {icon && <span className="dialog-type-icon">{icon}</span>}
                <h2>{title}</h2>
                <button type="button" className="icon-button" aria-label="关闭对话框" onClick={onClose}>
                    ×
                </button>
            </div>
            {children}
        </dialog>
    );
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
function iconFor(type: string | undefined) {
    if (type === "wait") return "clock";
    if (type === "keys" || type === "text") return "keyboard";
    if (type === "move" || type === "drag") return "cursor";
    if (type === "click" || type === "wheel") return "mouse";
    return "record";
}
export function EditorDialog(p: Props) {
    const action = p.target.action,
        kind = action?.kind;
    const [x, setX] = useState(kind && "x" in kind ? String(kind.x) : "0"),
        [y, setY] = useState(kind && "y" in kind ? String(kind.y) : "0");
    const [duration, setDuration] = useState(
        action ? String(Number(((action.endUs - action.startUs) / 1_000_000).toFixed(6))) : "1",
    );
    const [button, setButton] = useState<MouseButton>(kind?.type === "click" ? kind.button : "left");
    const [count, setCount] = useState(kind?.type === "click" ? kind.count : 1);
    const [steps, setSteps] = useState(kind?.type === "wheel" ? String(Math.abs(kind.delta) / 120) : "1");
    const [direction, setDirection] = useState(kind?.type === "wheel" && kind.delta < 0 ? -1 : 1);
    const [text, setText] = useState(kind?.type === "text" ? kind.text : "");
    const [chord, setChord] = useState(kind?.type === "keys" ? kind.chord : "");
    const [linked, setLinked] = useState(p.target.linkAvailable);
    const [localError, setLocalError] = useState("");
    useEffect(() => {
        if (p.pickedPoint) {
            setX(String(p.pickedPoint.x));
            setY(String(p.pickedPoint.y));
        }
    }, [p.pickedPoint]);
    const type = p.target.insert ?? kind?.type;
    const readOnly = action && !action.editable;
    const title = readOnly
        ? action.title
        : p.target.insert
          ? `插入${type === "wait" ? "等待" : "文本"}`
          : (action?.title ?? "编辑动作");
    const coordinateFields = (
        <>
            <div className="field-row">
                <label className="form-field">
                    X
                    <input type="number" step={1} required value={x} onChange={e => setX(e.target.value)} />
                </label>
                <label className="form-field">
                    Y
                    <input type="number" step={1} required value={y} onChange={e => setY(e.target.value)} />
                </label>
            </div>
            <p className="form-hint">相对于屏幕左上角</p>
            <button type="button" className="button pick-position" onClick={p.onPick}>
                <Icon name="cursor" size={16} />
                在预览中选位置
            </button>
        </>
    );
    return (
        <Modal title={title} onClose={p.onClose} open={!p.picking} icon={<Icon name={iconFor(type)} size={19} />}>
            <form
                className="dialog-form editor-form"
                onSubmit={e => {
                    e.preventDefault();
                    setLocalError("");
                    if (readOnly) {
                        p.onClose();
                        return;
                    }
                    const result = buildEditRequest(p.target, {
                        x,
                        y,
                        duration,
                        button,
                        count,
                        steps,
                        direction,
                        text,
                        chord,
                        linked,
                    });
                    if ("error" in result) {
                        setLocalError(result.error);
                        return;
                    }
                    p.onSave(result.edit);
                }}
            >
                <div className="dialog-body">
                    {readOnly ? (
                        <>
                            <p>{action.detail}</p>
                            <p className="form-note">
                                {kind?.type === "drag"
                                    ? "拖拽保留原始轨迹和按住顺序，暂不提供独立参数编辑。"
                                    : "此类输入可保留在录制中，暂不提供参数编辑。"}
                            </p>
                        </>
                    ) : (
                        <>
                            {type === "move" && (
                                <>
                                    {coordinateFields}
                                    <label className="form-field">
                                        移动耗时
                                        <span className="field-with-unit">
                                            <input
                                                type="number"
                                                min={0}
                                                step="any"
                                                required
                                                value={duration}
                                                onChange={e => setDuration(e.target.value)}
                                            />
                                            <span>秒</span>
                                        </span>
                                    </label>
                                    {p.target.linkAvailable && (
                                        <label className="check-field">
                                            <input
                                                type="checkbox"
                                                checked={linked}
                                                onChange={e => setLinked(e.target.checked)}
                                            />
                                            同时修改紧接着的点击位置
                                        </label>
                                    )}
                                </>
                            )}
                            {type === "click" && (
                                <>
                                    <div className="field-row">
                                        <label className="form-field">
                                            鼠标按键
                                            <select
                                                value={button}
                                                onChange={e => setButton(e.target.value as MouseButton)}
                                            >
                                                <option value="left">左键</option>
                                                <option value="right">右键</option>
                                                <option value="middle">中键</option>
                                            </select>
                                        </label>
                                        <label className="form-field">
                                            点击方式
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
                                <div className="field-row">
                                    <label className="form-field">
                                        滚动方向
                                        <select value={direction} onChange={e => setDirection(Number(e.target.value))}>
                                            <option value={1}>向上</option>
                                            <option value={-1}>向下</option>
                                        </select>
                                    </label>
                                    <label className="form-field">
                                        滚动量
                                        <span className="field-with-unit">
                                            <input
                                                type="number"
                                                min={1 / 120}
                                                step="any"
                                                required
                                                value={steps}
                                                onChange={e => setSteps(e.target.value)}
                                            />
                                            <span>格</span>
                                        </span>
                                    </label>
                                </div>
                            )}
                            {type === "wait" && (
                                <label className="form-field">
                                    等待时长
                                    <span className="field-with-unit">
                                        <input
                                            type="number"
                                            min={0.000001}
                                            step={0.000001}
                                            required
                                            autoFocus
                                            value={duration}
                                            onChange={e => setDuration(e.target.value)}
                                        />
                                        <span>秒</span>
                                    </span>
                                </label>
                            )}
                            {!p.target.insert && (type === "keys" || type === "text") && (
                                <>
                                    {type === "text" ? (
                                        <label className="form-field">
                                            输入内容
                                            <textarea
                                                autoFocus
                                                required
                                                rows={4}
                                                value={text}
                                                onChange={e => setText(e.target.value)}
                                            />
                                        </label>
                                    ) : (
                                        <label className="form-field">
                                            {chord.includes("+") ? "组合键" : "按键"}
                                            <input
                                                autoFocus
                                                required
                                                value={chord}
                                                placeholder="例如 Ctrl + S、Tab、A"
                                                onChange={e => setChord(e.target.value)}
                                            />
                                        </label>
                                    )}
                                </>
                            )}
                            {p.target.insert === "text" && (
                                <label className="form-field">
                                    输入内容
                                    <textarea
                                        autoFocus
                                        required
                                        rows={4}
                                        value={text}
                                        onChange={e => setText(e.target.value)}
                                    />
                                </label>
                            )}
                        </>
                    )}
                    {(localError || p.error) && (
                        <p className="form-error" role="alert">
                            {localError || p.error}
                        </p>
                    )}
                </div>
                <div className="dialog-footer">
                    <button type="button" className="button" onClick={p.onClose}>
                        取消
                    </button>
                    <button type="submit" className="button primary" disabled={p.busy}>
                        {readOnly ? "知道了" : p.busy ? "保存中…" : "确定"}
                    </button>
                </div>
            </form>
        </Modal>
    );
}
