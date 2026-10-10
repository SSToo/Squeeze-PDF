// Native WKWebView production frontend QA, with frozen/current frontend AND
// worker versions. The loopback IPC adapter is not a Tauri/WebView2 system test.
import { createServer } from "vite";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createHash } from "node:crypto";
import {
  mkdirSync,
  readFileSync,
  writeFileSync,
  appendFileSync,
  copyFileSync,
  rmSync,
  existsSync,
} from "node:fs";
import { resolve, join } from "node:path";
const exec = promisify(execFile),
  [corpusArg, rootArg, baselineArg] = process.argv.slice(2);
if (!baselineArg) throw Error("corpus output baseline-worker");
const root = resolve(rootArg),
  corpus = resolve(corpusArg),
  baseline = resolve(baselineArg);
mkdirSync(root, { recursive: true });
const files = (
  process.env.SQUEEZE_DESKTOP_CASES ||
  "vector-state,cid-font-plain,alpha-1-dark,cmyk-jpeg-normal"
)
  .split(",")
  .map((n) => join(corpus, n + ".pdf"));
const presets = (
  process.env.SQUEEZE_DESKTOP_PRESETS || "balanced,small,lossless"
).split(",");
const sessions = new Map(),
  records = [];
let serial = 0;
const hash = (b) => createHash("sha256").update(b).digest("hex");
async function body(req) {
  let size = 0;
  const chunks = [];
  for await (const c of req) {
    size += c.length;
    if (size > 80 * 1024 * 1024) throw Error("QA body budget");
    chunks.push(c);
  }
  return Buffer.concat(chunks);
}
const server = await createServer({
  configFile: false,
  root: resolve("."),
  server: {
    host: "127.0.0.1",
    port: 1427,
    strictPort: true,
    hmr: false,
    watch: { ignored: ["**/target/**", "**/tmp/**"] },
  },
  plugins: [
    {
      name: "advanced-qa",
      configureServer(server) {
        server.middlewares.use(async (req, res, next) => {
          if (!req.url.startsWith("/__advanced/")) return next();
          try {
            console.log("IPC", req.url, req.headers["x-qa-version"]);
            const cmd = req.url.slice("/__advanced/".length),
              b = await body(req);
            if (cmd === "log") {
              console.log("WK", b.toString());
              res.end("null");
              return;
            }
            if (cmd === "tests") {
              res.end(JSON.stringify({ files, presets }));
              return;
            }
            if (cmd === "read_pdf") {
              const { path } = JSON.parse(b);
              if (!files.includes(path)) throw Error("Unknown path");
              res.end(readFileSync(path));
              return;
            }
            if (cmd === "begin_raster") {
              const { path, preset } = JSON.parse(b);
              if (!files.includes(path) || !presets.includes(preset))
                throw Error("Invalid input");
              const plan = JSON.parse(
                (
                  await exec(resolve("target/release/examples/raster_plan"), [
                    path,
                    preset,
                  ])
                ).stdout,
              );
              const id = "session-" + serial++,
                dir = join(root, id);
              mkdirSync(dir);
              const expected = plan.pages.map(
                (p) => p.dimensions[0] * p.dimensions[1] * 3,
              );
              const version = req.headers["x-qa-version"];
              if (!["baseline", "latest"].includes(version))
                throw Error("Invalid version");
              sessions.set(id, {
                path,
                preset,
                version,
                dir,
                plan,
                expected,
                received: [expected.map(() => 0), expected.map(() => 0)],
                chunks: 0,
                max_chunk: 0,
              });
              res.end(JSON.stringify({ id, plan }));
              return;
            }
            if (cmd === "write_raster_page") {
              const s = sessions.get(req.headers["x-squeeze-session"]),
                index = Number(req.headers["x-squeeze-page"]),
                offset = Number(req.headers["x-squeeze-offset"] || 0),
                v = req.headers["x-squeeze-variant"] === "supersampled" ? 1 : 0;
              if (
                !s ||
                !Number.isInteger(index) ||
                index < 0 ||
                index >= s.expected.length ||
                offset !== s.received[v][index] ||
                !b.length ||
                offset + b.length > s.expected[index] ||
                ("x-squeeze-offset" in req.headers && b.length > 1024 * 1024)
              )
                throw Error("Pixel chunk invalid");
              if (v && s.received[0][index] !== s.expected[index])
                throw Error("Primary not complete");
              const dir = v ? join(s.dir, "supersampled") : s.dir;
              mkdirSync(dir, { recursive: true });
              const path = join(dir, index + ".rgb");
              if (!offset) writeFileSync(path, b, { flag: "wx" });
              else appendFileSync(path, b);
              s.received[v][index] += b.length;
              s.chunks++;
              s.max_chunk = Math.max(s.max_chunk, b.length);
              res.end("null");
              return;
            }
            const { id } = JSON.parse(b),
              s = sessions.get(id);
            if (!s) throw Error("Unknown session");
            if (cmd === "abort_raster") {
              if (!s.finished) {
                rmSync(s.dir, { recursive: true, force: true });
                s.aborted = true;
              }
              res.end("null");
              return;
            }
            if (
              cmd !== "finish_raster" ||
              s.received[0].some((n, i) => n !== s.expected[i]) ||
              s.received[1].some((n, i) => n && n !== s.expected[i])
            )
              throw Error("Incomplete session");
            const data = JSON.stringify({
              source_sha256: s.plan.source_sha256,
              pages: s.plan.pages.length,
            });
            const worker =
              s.version === "baseline"
                ? baseline
                : resolve("target/release/squeeze");
            async function encode(dir, name) {
              const manifest = join(dir, "manifest.json");
              writeFileSync(manifest, data);
              const output = join(s.dir, name + ".pdf");
              return JSON.parse(
                (
                  await exec(
                    worker,
                    [
                      "compress",
                      s.path,
                      "-o",
                      output,
                      "--preset",
                      s.preset,
                      "--output-mode",
                      "raster",
                      "--raster-manifest",
                      manifest,
                      "--json",
                    ],
                    { maxBuffer: 16 * 1024 * 1024 },
                  )
                ).stdout,
              );
            }
            let report = await encode(s.dir, "direct"),
              chosenDir = s.dir;
            const direct = report;
            let alternative, quality;
            if (s.received[1].some((n) => n)) {
              const dir = join(s.dir, "supersampled");
              for (let i = 0; i < s.expected.length; i++)
                if (!s.received[1][i])
                  copyFileSync(join(s.dir, i + ".rgb"), join(dir, i + ".rgb"));
              alternative = await encode(dir, "supersampled");
              if (alternative.result_bytes < report.result_bytes) {
                const flags = s.received[1].map((n) => n > 0);
                quality = JSON.parse(
                  (
                    await exec(
                      resolve("target/release/examples/raster_quality"),
                      [
                        report.output,
                        alternative.output,
                        s.dir,
                        dir,
                        JSON.stringify(flags),
                      ],
                    )
                  ).stdout,
                );
                if (quality.accepted) {
                  report = alternative;
                  chosenDir = dir;
                }
              }
            }
            if (hash(readFileSync(s.path)) !== s.plan.source_sha256)
              throw Error("Source changed");
            const row = {
              name: s.path.split("/").at(-1).slice(0, -4),
              input: s.path,
              input_sha256: s.plan.source_sha256,
              mode: "raster",
              preset: s.preset,
              version: s.version,
              ...report,
              desktop_rendering: {
                quality,
                selected: report === alternative,
                status:
                  report === alternative
                    ? "supersampled"
                    : alternative
                      ? "kept_direct"
                      : "direct",
                baseline_bytes: direct.result_bytes,
                candidate_bytes: alternative?.result_bytes,
                supersampled_pages: s.received[1].filter((n) => n > 0).length,
              },
              staging_directory: chosenDir,
              chunks: s.chunks,
              max_chunk: s.max_chunk,
            };
            records.push(row);
            s.finished = true;
            writeFileSync(
              join(s.dir, "result.json"),
              JSON.stringify(row, null, 2),
            );
            writeFileSync(
              join(root, "reports.json"),
              JSON.stringify(records, null, 2),
            );
            res.end(JSON.stringify(report));
          } catch (e) {
            res.statusCode = 500;
            res.end(String(e));
          }
        });
      },
    },
  ],
});
await server.listen();
const html = join(root, "qa.html");
writeFileSync(
  html,
  `<!doctype html><meta charset="utf-8"><script type="module">
window.requestAnimationFrame=f=>setTimeout(()=>f(performance.now()),0);window.cancelAnimationFrame=clearTimeout;
const log=x=>fetch('/__advanced/log',{method:'POST',body:JSON.stringify(x)});
let version='baseline',cancelOnce=false,cancel;
window.addEventListener('error',e=>log({error:e.message}));window.addEventListener('unhandledrejection',e=>log({rejection:String(e.reason)}));
window.__TAURI_INTERNALS__={invoke:async(cmd,args,options)=>{
 const raw=cmd==='write_raster_page';const res=await fetch('/__advanced/'+cmd,{method:'POST',body:raw?args:JSON.stringify(args),headers:{...(raw?options.headers:{'Content-Type':'application/json'}),'x-qa-version':version}});
 if(!res.ok)throw Error(await res.text());const result=cmd==='read_pdf'?await res.arrayBuffer():await res.json();if(raw&&cancelOnce){cancelOnce=false;cancel();}return result;
}};
try{const {files,presets}=await(await fetch('/__advanced/tests')).json(),q=new URLSearchParams(location.search),path=files[Number(q.get('case'))],preset=q.get('preset'),results=[];
for(version of ['baseline','latest']){
 const module=await import(/* @vite-ignore */ version==='baseline'?'/tmp/round9-remaining/baseline/source/src/rasterCompress.ts':'/src/rasterCompress.ts');cancel=module.cancelRaster;
 if(version==='latest'&&q.get('cancel')==='yes'){cancelOnce=true;let cancelled=false;try{await module.compressRaster(path,'qa',preset,log);}catch(e){cancelled=String(e).includes('取消');}if(!cancelled)throw Error('Cancellation did not reject');}
 // Two calls in one JS context exercise cleanup and retry; first is frozen.
 const report=await module.compressRaster(path,'qa',preset,log);results.push({version,bytes:report.result_bytes});
}
window.webkit.messageHandlers.qaResult.postMessage({ok:true,userAgent:navigator.userAgent,devicePixelRatio,results});
}catch(e){window.webkit.messageHandlers.qaResult.postMessage({ok:false,error:String(e),stack:e.stack});}
</script>`,
);
try {
  const cases = [];
  for (let i = 0; i < files.length; i++)
    for (const preset of presets) {
      const file = join(root, `webkit-${i}-${preset}.json`),
        url =
          "http://127.0.0.1:1427/" +
          html.slice(resolve(".").length + 1) +
          `?case=${i}&preset=${preset}&cancel=${i === 0 ? "yes" : "no"}`;
      await exec(
        resolve(
          process.env.SQUEEZE_WEBKIT_RUNNER ||
            "tmp/round9-remaining/run-webkit-qa",
        ),
        [url, file],
        {
          timeout: 240000,
        },
      );
      const result = JSON.parse(readFileSync(file));
      if (!result.ok) throw Error(JSON.stringify(result));
      cases.push(result);
      const current = records.at(-1),
        previous = records.at(-2);
      if (current.result_bytes > previous.result_bytes)
        throw Error("Desktop size regression");
      if (current.max_chunk > 1024 * 1024) throw Error("Chunk over budget");
      console.log("PASS", i, preset, current.desktop_rendering);
    }
  const aborted = [...sessions.values()].filter((s) => s.aborted);
  if (aborted.some((s) => existsSync(s.dir)))
    throw Error("Aborted staging leaked");
  writeFileSync(
    join(root, "webkit.json"),
    JSON.stringify(
      { ok: true, cases, aborted_cleaned: aborted.length },
      null,
      2,
    ),
  );
} finally {
  await server.close();
}
