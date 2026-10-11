import assert from "node:assert/strict";
import { test } from "node:test";
import { UpdateController } from "../src/updater.ts";

function fixture(options = {}) {
  const calls = [];
  let occupied = false;
  const handle = {
    version: "9.0.0",
    body: "Release notes <script>remain plain text</script>",
    async download(progress, config) {
      calls.push("download");
      assert.equal(config.timeout, 600_000);
      if (options.download) return options.download(progress);
      progress({
        event: "Started",
        data: { contentLength: options.unknownLength ? undefined : 100 },
      });
      progress({ event: "Progress", data: { chunkLength: 40 } });
      progress({ event: "Progress", data: { chunkLength: 60 } });
      progress({ event: "Finished" });
      if (options.badSignature) throw new Error("invalid signature");
    },
    async install() {
      calls.push("install");
      if (options.installError) throw new Error("install failed");
    },
    async close() {
      calls.push("close");
    },
  };
  let restartingFails = options.restartError;
  const controller = new UpdateController(
    {
      async check() {
        calls.push("check");
        return options.check ? options.check() : handle;
      },
      async relaunch() {
        calls.push("relaunch");
        if (restartingFails) {
          restartingFails = false;
          throw new Error("restart failed");
        }
      },
    },
    {
      acquire() {
        if (occupied) return false;
        occupied = true;
        calls.push("acquire");
        return true;
      },
      release() {
        occupied = false;
        calls.push("release");
      },
    },
  );
  return {
    controller,
    calls,
    handle,
    occupy(value) {
      occupied = value;
    },
    isOccupied: () => occupied,
  };
}

test("automatic detection never downloads or installs without user action", async () => {
  const { controller, calls } = fixture();
  await controller.check();
  assert.equal(controller.getSnapshot().phase, "available");
  assert.equal(controller.getSnapshot().version, "9.0.0");
  assert.deepEqual(calls, ["check"]);
});

test("offline automatic checks stay quiet; explicit checks report a retryable error", async () => {
  const { controller } = fixture({
    check() {
      throw new Error("offline");
    },
  });
  await controller.check();
  assert.equal(controller.getSnapshot().phase, "idle");
  assert.equal(controller.getSnapshot().error, undefined);
  await controller.check(true);
  assert.equal(controller.getSnapshot().error, "check");
});

test("overlapping checks are serialized and superseded resources are closed", async () => {
  let resolveCheck;
  const f = fixture({
    check: () =>
      new Promise((resolve) => {
        resolveCheck = resolve;
      }),
  });
  const pending = f.controller.check();
  await f.controller.check(true);
  assert.deepEqual(f.calls, ["check"]);
  resolveCheck(f.handle);
  await pending;
  const second = f.controller.check(true);
  resolveCheck(null);
  await second;
  assert.deepEqual(f.calls, ["check", "check", "close"]);
  assert.equal(f.controller.getSnapshot().phase, "current");
  assert.equal(f.controller.getSnapshot().version, undefined);
});

test("active PDF work blocks installation and restarting", async () => {
  const f = fixture();
  await f.controller.check();
  f.occupy(true);
  await f.controller.install();
  assert.deepEqual(f.calls, ["check"]);
  assert.equal(f.controller.getSnapshot().phase, "available");
});

test("signature failure never installs, releases the PDF lock and allows retry", async () => {
  const f = fixture({ badSignature: true });
  await f.controller.check();
  await f.controller.install();
  assert.deepEqual(f.calls, ["check", "acquire", "download", "release"]);
  assert.equal(f.controller.getSnapshot().error, "install");
  assert.equal(f.isOccupied(), false);
});

for (const unknownLength of [false, true])
  test(`verified download and install report progress (unknown length: ${unknownLength})`, async () => {
    const f = fixture({ unknownLength });
    const snapshots = [];
    const unsubscribe = f.controller.subscribe(() =>
      snapshots.push(f.controller.getSnapshot()),
    );
    await f.controller.check();
    await f.controller.install();
    unsubscribe();
    assert.equal(f.controller.getSnapshot().downloaded, 100);
    assert.equal(
      f.controller.getSnapshot().total,
      unknownLength ? undefined : 100,
    );
    assert(snapshots.some((state) => state.phase === "installing"));
    assert.deepEqual(f.calls, [
      "check",
      "acquire",
      "download",
      "install",
      "relaunch",
    ]);
    assert.equal(
      f.isOccupied(),
      true,
      "PDF work stays locked while the process restarts",
    );
  });

test("installation failures do not relaunch and leave the app usable", async () => {
  const f = fixture({ installError: true });
  await f.controller.check();
  await f.controller.install();
  assert(!f.calls.includes("relaunch"));
  assert.equal(f.controller.getSnapshot().error, "install");
  assert.equal(f.isOccupied(), false);
  await f.controller.install();
  assert.equal(
    f.calls.filter((call) => call === "download").length,
    1,
    "retry reuses verified bytes without leaking an earlier download resource",
  );
  assert.equal(f.calls.filter((call) => call === "install").length, 2);
});

test("a failed restart can be retried without downloading or installing again", async () => {
  const f = fixture({ restartError: true });
  await f.controller.check();
  await f.controller.install();
  assert.equal(f.controller.getSnapshot().error, "restart");
  assert.equal(f.isOccupied(), false);
  f.occupy(true);
  await f.controller.install();
  assert.equal(f.calls.filter((call) => call === "relaunch").length, 1);
  f.occupy(false);
  await f.controller.install();
  assert.equal(f.calls.filter((call) => call === "download").length, 1);
  assert.equal(f.calls.filter((call) => call === "install").length, 1);
  assert.equal(f.calls.filter((call) => call === "relaunch").length, 2);
});

test("repeated install clicks and checks cannot overlap an update", async () => {
  let finish;
  const f = fixture({
    download: () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  });
  await f.controller.check();
  const pending = f.controller.install();
  await f.controller.install();
  await f.controller.check(true);
  assert.deepEqual(f.calls, ["check", "acquire", "download"]);
  finish();
  await pending;
  assert.deepEqual(f.calls, [
    "check",
    "acquire",
    "download",
    "install",
    "relaunch",
  ]);
});
