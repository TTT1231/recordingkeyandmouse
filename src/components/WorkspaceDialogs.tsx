import { useState, useEffect, useRef } from "react";
import { Modal } from "./EditorDialog";
import { desktopBounds } from "../actions";
import { vkName } from "../format";
import type { DocumentView, Hotkeys, ReferenceImage } from "../types";
import { Icon } from "./Icon";
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
                        <kbd>Ctrl+Z / Ctrl+Shift+Z</kbd>
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
const stages = [
    {
        key: "tasks",
        label: "任务区域",
        title: "一个任务，保存一段录制",
        description: "在左侧新建或选择任务。每个任务都有自己的录制内容，切换任务即可继续查看。",
    },
    {
        key: "record",
        label: "连续录制",
        title: "按下开始，照常操作",
        description: "开始后会连续记录鼠标移动、点击、键盘输入，以及操作之间的停顿。完成后停止录制。",
    },
    {
        key: "edit",
        label: "回放与编辑",
        title: "先回放，需要时再修改",
        description: "单击动作可定位画面，双击打开编辑。右键动作可以在后面插入等待，再回放确认。",
    },
];
export function GuideDialog(p: { enterDemo: boolean; onClose: () => void; onFinish: () => void }) {
    const [stageIndex, setStageIndex] = useState(0);
    const ref = useRef<HTMLDialogElement>(null);
    const stage = stages[stageIndex];
    useEffect(() => {
        const node = ref.current;
        if (node && !node.open) node.showModal();
        return () => {
            if (node?.open) node.close();
        };
    }, []);
    return (
        <dialog
            ref={ref}
            className="tour-dialog"
            aria-labelledby="tour-title"
            aria-describedby="tour-description"
            onCancel={e => {
                e.preventDefault();
                p.onClose();
            }}
        >
            <header className="tour-header">
                <h2 className="tour-title" id="tour-title">
                    先认识一下工作区
                </h2>
                <button type="button" className="tour-close" aria-label="关闭引导" onClick={p.onClose}>
                    <Icon name="x" size={18} />
                </button>
            </header>
            <nav className="tour-stage-nav" aria-label="引导主题">
                {stages.map((item, index) => (
                    <button
                        key={item.key}
                        type="button"
                        className={`tour-stage-button${index === stageIndex ? " tour-stage-current" : ""}`}
                        aria-current={index === stageIndex ? "true" : undefined}
                        onClick={() => setStageIndex(index)}
                    >
                        {item.label}
                    </button>
                ))}
            </nav>
            <div className="tour-map" data-tour-region={stage.key} aria-hidden="true">
                <div className="tour-map-titlebar">
                    <span className="tour-map-brand">Repeat</span>
                    <span>每日录入</span>
                    <span className="tour-map-window">—　□　×</span>
                </div>
                <aside className="tour-map-tasks">
                    <div className="tour-map-task-heading">
                        <strong>任务</strong>
                        <span>＋</span>
                    </div>
                    <div className="tour-map-task tour-map-task-selected">
                        <Icon name="folder" size={13} />
                        <span>每日录入</span>
                    </div>
                    <div className="tour-map-task">
                        <Icon name="folder" size={13} />
                        <span>文件整理</span>
                    </div>
                </aside>
                <div className="tour-map-workspace">
                    <div className="tour-map-toolbar">
                        <span className="tour-map-record">
                            <i />
                            <span className="tour-map-record-idle">开始录制</span>
                            <span className="tour-map-record-active">正在录制</span>
                        </span>
                        <span className="tour-map-play">▷　回放</span>
                        <span className="tour-map-timer">00:03</span>
                    </div>
                    <div className="tour-map-preview">
                        <div className="tour-map-document">
                            <span />
                            <span />
                            <span />
                        </div>
                        <span className="tour-map-cursor">
                            <Icon name="cursor" size={17} />
                        </span>
                    </div>
                    <div className="tour-map-timeline">
                        <div className="tour-map-ruler">
                            <span>0:00</span>
                            <span>0:02</span>
                            <span>0:04</span>
                        </div>
                        <div className="tour-map-clip">
                            <span className="tour-map-motion">
                                <Icon name="cursor" size={11} />
                                <i />
                            </span>
                            <span className="tour-map-key">B</span>
                            <span className="tour-map-key tour-map-key-shortcut">Ctrl + S</span>
                            <span className="tour-map-playhead" />
                        </div>
                    </div>
                </div>
            </div>
            <section className="tour-explanation" aria-live="polite">
                <h3 className="tour-stage-title">{stage.title}</h3>
                <p className="tour-description" id="tour-description">
                    {stage.description}
                </p>
            </section>
            <footer className="tour-footer">
                <button type="button" className="tour-skip" onClick={p.onFinish}>
                    跳过引导
                </button>
                <div className="tour-footer-actions">
                    <button
                        type="button"
                        className="tour-button tour-back"
                        disabled={stageIndex === 0}
                        onClick={() => setStageIndex(index => Math.max(0, index - 1))}
                    >
                        上一步
                    </button>
                    <button
                        type="button"
                        className="tour-button tour-next"
                        onClick={() =>
                            stageIndex === stages.length - 1 ? p.onFinish() : setStageIndex(stageIndex + 1)
                        }
                    >
                        {stageIndex === stages.length - 1 ? (p.enterDemo ? "进入使用演示" : "完成") : "下一项"}
                    </button>
                </div>
            </footer>
        </dialog>
    );
}
