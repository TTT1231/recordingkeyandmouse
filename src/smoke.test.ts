import { describe, expect, it } from "vitest";
import App from "./App";

// 工具链冒烟测试：只验证 vitest 能编译并加载前端模块，不表达业务行为。
// 出现真实业务逻辑后，按测试约定补充对应测试并取代本文件。
describe("scaffold smoke", () => {
    it("imports the App module", () => {
        expect(App).toBeTypeOf("function");
    });
});
