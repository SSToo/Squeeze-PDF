import assert from "node:assert/strict";
import { test } from "node:test";
import { catalog } from "../src/i18n/catalog.ts";
import {
  createTranslator,
  detectLanguage,
  languages,
} from "../src/i18n/locale.ts";

const tokens = (text) =>
  [...text.matchAll(/\{([a-z]+)\}/g)].map((match) => match[1]).sort();

test("all eight languages have complete messages with matching placeholders", () => {
  assert.deepEqual(
    languages.map(({ id }) => id),
    ["en", "zh", "fr", "es", "ko", "ja", "de", "pt"],
  );
  for (const [key, translations] of Object.entries(catalog)) {
    assert.equal(translations.length, 7, key);
    for (const text of translations) {
      assert.ok(text.trim(), key);
      assert.deepEqual(tokens(text), tokens(key), key);
    }
    for (const { id } of languages) {
      const values = Object.fromEntries(
        tokens(key).map((token) => [token, "VALUE"]),
      );
      assert.deepEqual(
        tokens(createTranslator(id).t(key, values)),
        [],
        `${id}: ${key}`,
      );
    }
  }
});

test("saved and regional language preferences resolve with a supported fallback", () => {
  assert.equal(detectLanguage("pt-BR", ["zh-CN"]), "pt");
  assert.equal(detectLanguage(null, ["it-IT", "fr-CA", "en"]), "fr");
  assert.equal(detectLanguage("invalid", ["KO_kr"]), "ko");
  assert.equal(detectLanguage(null, ["zh-Hant"]), "zh");
  assert.equal(detectLanguage(null, ["ja-JP"]), "ja");
  assert.equal(detectLanguage(null, ["unsupported"]), "en");
});

test("live worker phases, wrapped JavaScript errors and report warnings translate", () => {
  for (const { id } of languages.filter(({ id }) => id !== "zh")) {
    const { t, raw } = createTranslator(id);
    assert.equal(
      raw("整页渲染为图片 8 / 52"),
      t("整页渲染为图片 {page} / {total}", { page: 8, total: 52 }),
    );
    assert.equal(
      raw("Error: 第 8 页存在无法解码的图片或字体，未导出不完整页面"),
      "Error: " +
        t("第 {page} 页存在无法解码的图片或字体，未导出不完整页面", {
          page: 8,
        }),
    );
    assert.equal(
      raw("暂不支持：加密 PDF"),
      t("暂不支持：{detail}", { detail: t("加密 PDF") }),
    );
    assert.equal(
      raw("Error: 第 3 页的批注或表单无法完整转为图片，请使用保留原始内容模式"),
      "Error: " +
        t("第 {page} 页的批注或表单无法完整转为图片，请使用保留原始内容模式", {
          page: 3,
        }),
    );
    assert.equal(
      raw("已保留 2 个嵌入附件，附件数据未重新编码"),
      t("已保留 {count} 个嵌入附件，附件数据未重新编码", { count: 2 }),
    );
    assert.equal(
      raw("已复用 12345 组矢量轮廓，重复形状共享矢量对象；请检查细字与边缘"),
      t("已复用 {count} 组矢量轮廓，重复形状共享矢量对象；请检查细字与边缘", {
        count: 12345,
      }),
    );
    assert.equal(
      raw("完成：2 · 失败：1。失败原因显示在对应文件下方，可点击重试。"),
      t(
        "完成：{ok} · 失败：{failed}。失败原因显示在对应文件下方，可点击重试。",
        { ok: 2, failed: 1 },
      ),
    );
    assert.equal(
      raw("任务已取消\n页面会话已结束"),
      t("任务已取消") + "\n" + t("页面会话已结束"),
    );
    for (const suffix of ["", "，超采样抗锯齿"]) {
      const rendered = raw(
        `第 8 页整页渲染为图片，宽 1400 像素，无损 Flate${suffix}`,
      );
      assert.ok(
        !/\p{Script=Han}/u.test(id === "ja" ? "" : rendered),
        `${id}: ${rendered}`,
      );
      assert.ok(!rendered.includes("{sampling}"));
    }
  }
});

test("file names, paths, unknown OS details and interpolation values stay intact", () => {
  const path = "C:\\报告\\任务已取消 [a]+$中文.pdf";
  const name = "中文 {count} $&.pdf";
  for (const { id } of languages) {
    const { t, raw } = createTranslator(id);
    assert.ok(t("移除 {name}", { name }).includes(name));
    assert.ok(raw(`输出文件已存在，未覆盖：${path}`).includes(path));
    assert.ok(
      raw(`文件操作失败：Permission denied: ${path}`).includes(
        `Permission denied: ${path}`,
      ),
    );
    assert.equal(
      raw("Unknown library error [42]"),
      "Unknown library error [42]",
    );
    assert.equal(raw(`Unexpected: ${path}`), `Unexpected: ${path}`);
  }
});

test("numbers, file sizes and percentages use the selected locale", () => {
  assert.equal(createTranslator("en").number(1400), "1,400");
  assert.equal(createTranslator("de").number(1400), "1.400");
  assert.equal(createTranslator("de").bytes(1.25 * 1048576), "1,25 MB");
  assert.equal(createTranslator("fr").bytes(1048576), "1,00 Mo");
  assert.equal(createTranslator("fr").bytes(1024), "1,0 Ko");
  assert.equal(createTranslator("en").percent(0.965, 1), "96.5%");
  assert.equal(createTranslator("de").percent(0.965, 1), "96,5\u00a0%");
});
