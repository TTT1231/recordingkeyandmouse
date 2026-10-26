import type { ExecutionReportView } from "../types";

interface Props {
    report: ExecutionReportView;
    onExport: () => void;
}

/** 执行报告：结果、失败原因、收尾释放、偏差统计（M0-03 测量字段，内部自检目标）。 */
export function ExecutionReportPanel({ report, onExport }: Props) {
    const { outcome, stats } = report;
    const stoppedText = outcome.failure ? "失败" : outcome.stopped ? "已停止" : "完成";
    const verdict = outcome.failure
        ? `第 ${outcome.failure.round} 轮失败：${outcome.failure.reason}`
        : outcome.stopped
          ? "用户停止"
          : "全部轮次完成";

    return (
        <div className="report">
            <h3>上次执行报告</h3>
            <dl>
                <dt>结果</dt>
                <dd>
                    {stoppedText} —— {verdict}
                </dd>
                <dt>轮次</dt>
                <dd>
                    完成 {outcome.roundsCompleted} / 计划 {outcome.roundsRequested} 轮
                </dd>
                <dt>注入事件</dt>
                <dd>{outcome.eventsInjected} 条</dd>
                {outcome.stopObservedAfterUs !== null && (
                    <>
                        <dt>停止响应</dt>
                        <dd>停止信号到调度确认 {(outcome.stopObservedAfterUs / 1000).toFixed(1)}ms</dd>
                    </>
                )}
                {outcome.released.length > 0 && (
                    <>
                        <dt>收尾释放</dt>
                        <dd>{outcome.released.join("、")}</dd>
                    </>
                )}
                {stats.sampleCount > 0 && (
                    <>
                        <dt>注入偏差</dt>
                        <dd>
                            P95 {(stats.p95AbsUs / 1000).toFixed(2)}ms · P99 {(stats.p99AbsUs / 1000).toFixed(2)}ms ·
                            最大 {(stats.maxAbsUs / 1000).toFixed(2)}ms
                        </dd>
                        <dt>累计漂移</dt>
                        <dd>
                            {(stats.driftUs / 1000).toFixed(2)}ms（共 {stats.sampleCount} 条注入）
                        </dd>
                    </>
                )}
            </dl>
            <button type="button" onClick={onExport}>
                导出执行报告 JSON
            </button>
            <p className="muted">严重落后时停止发送输入，当前阈值为 {outcome.lateThresholdMs}ms。</p>
        </div>
    );
}
