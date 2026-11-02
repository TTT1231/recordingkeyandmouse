// 更新检查：对比 GitHub Releases 最新发布版与当前版本。
// 仅检查并提醒，不下载安装；只发送 GET 请求，不上传任何本地数据。

/** Releases 托管仓库；迁移仓库或有多个发行渠道时改这里。 */
export const RELEASE_REPO = "TTT1231/recordingkeyandmouse";

/** 检查更新一次的超时时间。 */
const UPDATE_TIMEOUT_MS = 10_000;

export interface ReleaseInfo {
    /** 版本号：tag 去掉 v 前缀与后缀后的数值主段，如 "0.2.0"。 */
    version: string;
    /** Release 页面地址，用于「前往下载」。 */
    url: string;
    /** Release 说明原文，纯文本展示。 */
    notes: string;
}

/** 取版本号的数值主段："v1.2.3-beta.1+build" → "1.2.3"。 */
function coreVersion(value: string): string {
    return value.trim().replace(/^[vV]/, "").split(/[-+]/)[0];
}

/** candidate 是否比 current 更新；候选任一段非数字（nightly、1.2.x）显式视为不更新。 */
export function isNewerVersion(current: string, candidate: string): boolean {
    const currentSegments = coreVersion(current).split(".").map(Number);
    const candidateSegments = coreVersion(candidate).split(".").map(Number);
    if (candidateSegments.some(segment => !Number.isInteger(segment))) return false;
    const length = Math.max(currentSegments.length, candidateSegments.length);
    for (let index = 0; index < length; index += 1) {
        const currentSegment = currentSegments[index] ?? 0;
        const candidateSegment = candidateSegments[index] ?? 0;
        if (currentSegment !== candidateSegment) return candidateSegment > currentSegment;
    }
    return false;
}

/** 把网络与 HTTP 失败映射成可直接展示的中文原因。 */
async function fetchLatestRelease(): Promise<unknown> {
    let response: Response;
    try {
        response = await fetch(`https://api.github.com/repos/${RELEASE_REPO}/releases/latest`, {
            headers: { Accept: "application/vnd.github+json" },
            signal: AbortSignal.timeout(UPDATE_TIMEOUT_MS),
        });
    } catch (e) {
        if (e instanceof Error && e.name === "TimeoutError") throw new Error("连接超时，请稍后重试");
        if (e instanceof TypeError) throw new Error("无法连接 GitHub，请检查网络");
        throw new Error("检查更新失败，请稍后重试");
    }
    // /releases/latest 按定义排除 prerelease 与 draft；仓库没有已发布 Release 时也是 404。
    if (response.status === 404) return null;
    if (!response.ok) {
        if (response.status === 403 || response.status === 429) throw new Error("请求过于频繁，请稍后再试");
        throw new Error(`GitHub 返回 ${response.status}`);
    }
    return response.json();
}

/**
 * 检查 GitHub 最新发布版。返回 null 表示已是最新（含仓库暂无 Release）；
 * 网络或数据问题时抛出带中文原因的 Error。
 */
export async function checkForUpdate(currentVersion: string): Promise<ReleaseInfo | null> {
    const release = (await fetchLatestRelease()) as { tag_name?: unknown; html_url?: unknown; body?: unknown } | null;
    if (!release) return null;
    const tag = typeof release.tag_name === "string" ? release.tag_name : "";
    const url = typeof release.html_url === "string" ? release.html_url : "";
    if (!tag || !url) throw new Error("GitHub 返回数据不完整");
    if (!isNewerVersion(currentVersion, tag)) return null;
    return { version: coreVersion(tag), url, notes: typeof release.body === "string" ? release.body : "" };
}
