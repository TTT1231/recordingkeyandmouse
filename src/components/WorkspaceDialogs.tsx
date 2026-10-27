import { useState } from "react";
import { Modal } from "./EditorDialog";
import { desktopBounds } from "../actions";
import { vkName } from "../format";
import type { DocumentView, Hotkeys, ReferenceImage } from "../types";
export function NameDialog(p: {
    mode: "create" | "rename";
    initial: string;
    busy: boolean;
    error: string | null;
    onClose: () => void;
    onSave: (name: string) => void;
}) {
    const [name, setName] = useState(p.initial);
    return (
        <Modal title={p.mode === "create" ? "新建任务" : "重命名任务"} onClose={p.onClose}>
            <form
                className="dialog-form"
                onSubmit={e => {
                    e.preventDefault();
                    p.onSave(name);
                }}
            >
                <label className="form-field">
                    任务名称
                    <input
                        autoFocus
                        required
                        maxLength={60}
                        placeholder="为这项重复操作起个名字"
                        value={name}
                        onFocus={e => e.target.select()}
                        onChange={e => setName(e.target.value)}
                    />
                </label>
                {p.error && (
                    <p className="form-error" role="alert">
                        {p.error}
                    </p>
                )}
                <div className="dialog-actions">
                    <button type="button" className="button" onClick={p.onClose}>
                        取消
                    </button>
                    <button type="submit" className="button primary" disabled={p.busy}>
                        {p.mode === "create" ? "新建" : "保存"}
                    </button>
                </div>
            </form>
        </Modal>
    );
}
export function ReferenceDialog(p: {
    document: DocumentView;
    busy: boolean;
    error: string | null;
    onClose: () => void;
    onSave: (reference: ReferenceImage | null) => void;
}) {
    const [draft, setDraft] = useState<ReferenceImage | null>(p.document.referenceImage),
        [error, setError] = useState("");
    const bounds = desktopBounds(p.document.geometry.monitors);
    const load = async (file: File) => {
        setError("");
        if (file.size > 20 * 1024 * 1024) {
            setError("请选择不超过 20 MB 的图片");
            return;
        }
        if (!["image/png", "image/jpeg", "image/webp"].includes(file.type)) {
            setError("请选择 PNG、JPEG 或 WebP 图片");
            return;
        }
        try {
            const dataUrl = await new Promise<string>((resolve, reject) => {
                const reader = new FileReader();
                reader.onload = () => resolve(String(reader.result));
                reader.onerror = () => reject(new Error("读取图片失败"));
                reader.readAsDataURL(file);
            });
            const image = new Image();
            await new Promise<void>((resolve, reject) => {
                image.onload = () => resolve();
                image.onerror = () => reject(new Error("无法识别图片内容"));
                image.src = dataUrl;
            });
            setDraft({
                name: file.name,
                dataUrl,
                width: image.naturalWidth,
                height: image.naturalHeight,
                mapping: bounds,
            });
        } catch (e) {
            setError(String(e));
        }
    };
    return (
        <Modal title="预览背景" onClose={p.onClose}>
            <form
                className="dialog-form"
                onSubmit={e => {
                    e.preventDefault();
                    p.onSave(draft);
                }}
            >
                <p className="field-help">手动导入图片作为预览背景。图片不代表录制时的软件画面，也不会自动识别控件。</p>
                <label>
                    选择图片
                    <input
                        type="file"
                        accept="image/png,image/jpeg,image/webp"
                        onChange={e => {
                            const file = e.target.files?.[0];
                            if (file) void load(file);
                        }}
                    />
                </label>
                {draft && (
                    <>
                        <img className="reference-thumbnail" src={draft.dataUrl} alt={`预览背景：${draft.name}`} />
                        <p className="muted">
                            {draft.name} · {draft.width} × {draft.height}
                        </p>
                        <details>
                            <summary>图片对应的桌面范围</summary>
                            <p className="field-help">
                                默认对应整张桌面。图片会等比显示；裁剪过的截图可填写其实际桌面范围。
                            </p>
                            <div className="field-row">
                                {(["x", "y", "width", "height"] as const).map(key => (
                                    <label key={key}>
                                        {{ x: "左侧 X", y: "顶部 Y", width: "范围宽度", height: "范围高度" }[key]}
                                        <input
                                            type="number"
                                            required
                                            step={1}
                                            min={key === "width" || key === "height" ? 1 : undefined}
                                            value={draft.mapping[key]}
                                            onChange={e =>
                                                setDraft({
                                                    ...draft,
                                                    mapping: { ...draft.mapping, [key]: Number(e.target.value) },
                                                })
                                            }
                                        />
                                    </label>
                                ))}
                            </div>
                        </details>
                        <button type="button" className="text-button danger-text" onClick={() => setDraft(null)}>
                            移除预览背景
                        </button>
                    </>
                )}
                {(error || p.error) && (
                    <p className="form-error" role="alert">
                        {error || p.error}
                    </p>
                )}
                <div className="dialog-actions">
                    <button type="button" className="button" onClick={p.onClose}>
                        取消
                    </button>
                    <button type="submit" className="button primary" disabled={p.busy}>
                        确认
                    </button>
                </div>
            </form>
        </Modal>
    );
}
export function ShortcutsDialog(p: {
    hotkeys: Hotkeys;
    busy: boolean;
    error: string | null;
    onClose: () => void;
    onSave: (hotkeys: Hotkeys) => void;
}) {
    const [keys, setKeys] = useState(p.hotkeys);
    const options = Array.from({ length: 12 }, (_, i) => 0x70 + i);
    return (
        <Modal title="快捷键设置" onClose={p.onClose}>
            <form
                className="dialog-form"
                onSubmit={e => {
                    e.preventDefault();
                    p.onSave(keys);
                }}
            >
                <label>
                    全局开始 / 停止录制
                    <select
                        value={keys.recordVk}
                        onChange={e => setKeys({ ...keys, recordVk: Number(e.target.value) })}
                    >
                        {options.map(vk => (
                            <option key={vk} value={vk}>
                                {vkName(vk)}
                            </option>
                        ))}
                    </select>
                </label>
                <label>
                    全局停止系统执行
                    <select value={keys.stopVk} onChange={e => setKeys({ ...keys, stopVk: Number(e.target.value) })}>
                        {options.map(vk => (
                            <option key={vk} value={vk}>
                                {vkName(vk)}
                            </option>
                        ))}
                    </select>
                </label>
                <p className="field-help">两个快捷键应不同。若已被其他应用占用，会保留原设置并提示。</p>
                <div className="shortcut-list">
                    <div>
                        <span>撤销 / 重做</span>
                        <kbd>Ctrl+Z / Ctrl+Y</kbd>
                    </div>
                    <div>
                        <span>编辑选中动作</span>
                        <kbd>F2</kbd>
                    </div>
                    <div>
                        <span>保存任务</span>
                        <kbd>Ctrl+S</kbd>
                    </div>
                </div>
                {p.error && (
                    <p className="form-error" role="alert">
                        {p.error}
                    </p>
                )}
                <div className="dialog-actions">
                    <button type="button" className="button" onClick={p.onClose}>
                        取消
                    </button>
                    <button type="submit" className="button primary" disabled={p.busy}>
                        确认
                    </button>
                </div>
            </form>
        </Modal>
    );
}
