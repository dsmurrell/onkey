// npm test
import { test } from "node:test";
import assert from "node:assert/strict";
import { fileName, isLinuxAgent, savedFolder, isWindowsAgent, onWindows, pcWords, setupSteps } from "./platform.ts";

test("WebView2 on Windows is Windows; the Mac's WebKit and anything unknown is the Mac", () => {
  assert.ok(isWindowsAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0"));
  assert.ok(!isWindowsAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)"));
  assert.ok(!isWindowsAgent(""));
  assert.ok(!isWindowsAgent(undefined));
});

test("no window (the page being built): the Mac", () => {
  if (typeof navigator === "undefined" || !/Windows/.test(navigator.userAgent ?? "")) assert.equal(onWindows(), false);
});

test("the Mac's setup is as it was; Windows skips what it has no permission for", () => {
  assert.deepEqual([...setupSteps(false)], ["Welcome", "Permissions", "Music apps", "Mic check", "Ready"]);
  assert.deepEqual([...setupSteps(true)], ["Welcome", "Mic check", "Ready"]);
});

test("a saved report's file name, from a Windows or a Mac path", () => {
  assert.equal(fileName("~\\Desktop\\OnKey-report-2026-10-28-101500.zip"), "OnKey-report-2026-10-28-101500.zip");
  assert.equal(fileName("~/Desktop/OnKey-report-2026-10-28.zip"), "OnKey-report-2026-10-28.zip");
  assert.equal(fileName("report.zip"), "report.zip");
});

test("WebKitGTK on Linux is Linux, and a PC; Windows and the Mac aren't Linux", () => {
  assert.ok(isLinuxAgent("Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko)"));
  assert.ok(!isLinuxAgent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0"));
  assert.ok(!isLinuxAgent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko)"));
  assert.ok(!isLinuxAgent("Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36"));
  assert.ok(!isLinuxAgent(undefined));
});

test("Windows' words are as they were; Linux names its own", () => {
  assert.equal(pcWords(false).soundInput, "Windows Settings → System → Sound → Input");
  assert.equal(pcWords(false).files, "File Explorer");
  assert.equal(pcWords(true).os, "Linux");
  assert.ok(!Object.values(pcWords(true)).some(w => w.includes("Windows")));
});

test("where a report was saved, in words", () => {
  assert.equal(savedFolder("~\\Desktop\\OnKey-report-2026-10-28-101500.zip"), "your Desktop");
  assert.equal(savedFolder("~/Desktop/OnKey-report.zip"), "your Desktop");
  assert.equal(savedFolder("~/OnKey-report.zip"), "your home folder");
  // Linux names the Desktop in its own language (XDG): a folder known only by name is said as one
  assert.equal(savedFolder("~/Bureau/OnKey-report.zip"), "the folder Bureau");
  assert.equal(savedFolder("/home/ann/Рабочий стол/OnKey-report.zip"), "the folder Рабочий стол");
});
