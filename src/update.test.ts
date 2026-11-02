import { afterEach, describe, expect, it, vi } from "vitest";

import { RELEASE_REPO, checkForUpdate, isNewerVersion } from "./update";

describe("isNewerVersion", () => {
    it("相等版本不算更新", () => {
        expect(isNewerVersion("0.1.0", "0.1.0")).toBe(false);
    });

    it("识别更高的主/次/补丁版本", () => {
        expect(isNewerVersion("0.1.0", "0.1.1")).toBe(true);
        expect(isNewerVersion("0.1.0", "0.2.0")).toBe(true);
        expect(isNewerVersion("0.1.0", "1.0.0")).toBe(true);
        expect(isNewerVersion("1.0.0", "0.9.9")).toBe(false);
    });

    it("剥掉 v 前缀与构建元数据", () => {
        expect(isNewerVersion("0.1.0", "v0.2.0")).toBe(true);
        expect(isNewerVersion("0.1.0", "V0.2.0+build.5")).toBe(true);
        expect(isNewerVersion("0.2.0", "v0.2.0+build.5")).toBe(false);
    });

    it("段数不齐时缺段补 0", () => {
        expect(isNewerVersion("0.1", "0.1.0")).toBe(false);
        expect(isNewerVersion("0.1.0", "0.2")).toBe(true);
    });

    it("prerelease 只比较数值主段：不高于同名正式版", () => {
        expect(isNewerVersion("0.2.0", "0.2.0-beta.1")).toBe(false);
        expect(isNewerVersion("0.1.0", "0.2.0-rc.1")).toBe(true);
    });

    it("非数字段显式判为不更新", () => {
        expect(isNewerVersion("0.1.0", "nightly")).toBe(false);
        expect(isNewerVersion("0.1.0", "1.2.x")).toBe(false);
    });
});

describe("checkForUpdate", () => {
    afterEach(() => {
        vi.unstubAllGlobals();
    });

    const release = {
        tag_name: "v0.2.0",
        html_url: `https://github.com/${RELEASE_REPO}/releases/tag/v0.2.0`,
        body: "修复若干问题。",
    };

    it("发现新版本时返回 Release 信息", async () => {
        const fetchMock = vi.fn(async () => new Response(JSON.stringify(release), { status: 200 }));
        vi.stubGlobal("fetch", fetchMock);
        await expect(checkForUpdate("0.1.0")).resolves.toEqual({
            version: "0.2.0",
            url: release.html_url,
            notes: "修复若干问题。",
        });
        expect(fetchMock).toHaveBeenCalledWith(
            expect.stringContaining(`repos/${RELEASE_REPO}/releases/latest`),
            expect.anything(),
        );
    });

    it("版本不比当前新时返回 null", async () => {
        vi.stubGlobal(
            "fetch",
            vi.fn(async () => new Response(JSON.stringify(release), { status: 200 })),
        );
        await expect(checkForUpdate("0.2.0")).resolves.toBeNull();
    });

    it("body 为 null 时 notes 是空字符串", async () => {
        vi.stubGlobal(
            "fetch",
            vi.fn(async () => new Response(JSON.stringify({ ...release, body: null }), { status: 200 })),
        );
        const info = await checkForUpdate("0.1.0");
        expect(info?.notes).toBe("");
    });

    it("仓库没有已发布 Release（404）时返回 null", async () => {
        vi.stubGlobal(
            "fetch",
            vi.fn(async () => new Response("Not Found", { status: 404 })),
        );
        await expect(checkForUpdate("0.1.0")).resolves.toBeNull();
    });

    it("HTTP 500 抛出带状态码的中文错误", async () => {
        vi.stubGlobal(
            "fetch",
            vi.fn(async () => new Response("oops", { status: 500 })),
        );
        await expect(checkForUpdate("0.1.0")).rejects.toThrow("GitHub 返回 500");
    });

    it("限流（403）抛出中文错误", async () => {
        vi.stubGlobal(
            "fetch",
            vi.fn(async () => new Response("rate limited", { status: 403 })),
        );
        await expect(checkForUpdate("0.1.0")).rejects.toThrow("请求过于频繁");
    });
});
