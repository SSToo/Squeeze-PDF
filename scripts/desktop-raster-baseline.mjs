// Executes the real compressRaster frontend in a native WKWebView. Only IPC is
// adapted to a loopback QA service; both frozen/current Rust workers consume
// the same staging protocol. This is not an installed-app or Windows UI test.
import { createServer } from "vite";
import { execFile } from "node:child_process";
import { promisify } from "node:util";
import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve, join } from "node:path";
const exec = promisify(execFile);
const [corpusArg, rootArg, baselineArg] = process.argv.slice(2);
if (!baselineArg) throw Error("corpus output-directory baseline-worker");
const root = resolve(rootArg),
  corpus = resolve(corpusArg),
  baseline = resolve(baselineArg);
mkdirSync(root, { recursive: true });
const files = (process.env.SQUEEZE_DESKTOP_CASES?.split(",") || [
  "lzw-early0-content",
  "ascii85-jpeg-image",
  "softmask-matte",
  "appearance-protected-wrapper",
  "short-contents-state",
  "hex-flate-predictor-image",
]).map((n) => join(corpus, n + ".pdf"));
const allowed = new Set(files),
  sessions = new Map();
let next = 0;
const hash = (b) => createHash("sha256").update(b).digest("hex");
async function readBody(req) {
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > 80 * 1024 * 1024) throw Error("QA body limit");
    chunks.push(chunk);
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
      name: "desktop-raster-qa",
      configureServer(server) {
        server.middlewares.use(async (req, res, nextHandler) => {
          if (!req.url.startsWith("/__rasterqa/")) return nextHandler();
          try {
            if (req.url === "/__rasterqa/tests") {
              res.setHeader("Content-Type", "application/json");
              res.end(JSON.stringify(files));
              return;
            }
            const cmd = req.url.slice("/__rasterqa/".length),
              body = await readBody(req);
            if (cmd === "log") {
              console.log("WEBKIT", body.toString());
              res.end("null");
              return;
            }
            console.log("IPC", cmd);
            if (cmd === "read_pdf") {
              const { path } = JSON.parse(body);
              if (!allowed.has(path)) throw Error("Unknown fixture");
              res.setHeader("Content-Type", "application/octet-stream");
              res.end(readFileSync(path));
              return;
            }
            if (cmd === "begin_raster") {
              const { path, preset } = JSON.parse(body);
              if (
                !allowed.has(path) ||
                !["lossless", "balanced", "small"].includes(preset)
              )
                throw Error("Invalid plan request");
              const { stdout } = await exec(
                resolve("target/release/examples/raster_plan"),
                [path, preset],
              );
              const plan = JSON.parse(stdout),
                id = "qa-" + next++,
                dir = join(root, id);
              mkdirSync(dir);
              sessions.set(id, { path, preset, plan, dir, received: 0 });
              res.end(JSON.stringify({ id, plan }));
              return;
            }
            if (cmd === "write_raster_page") {
              const id = req.headers["x-squeeze-session"],
                index = Number(req.headers["x-squeeze-page"]),
                s = sessions.get(id);
              if (!s || index !== s.received) throw Error("Invalid page order");
              const [w, h] = s.plan.pages[index].dimensions;
              if (body.length !== w * h * 3) throw Error("Invalid RGB size");
              writeFileSync(join(s.dir, index + ".rgb"), body, { flag: "wx" });
              s.received++;
              res.end("null");
              return;
            }
            const { id } = JSON.parse(body),
              s = sessions.get(id);
            if (!s) throw Error("Unknown QA session");
            if (cmd === "abort_raster") {
              res.end("null");
              return;
            }
            if (cmd !== "finish_raster" || s.received !== s.plan.pages.length)
              throw Error("Incomplete session");
            const manifest = join(s.dir, "manifest.json");
            writeFileSync(
              manifest,
              JSON.stringify({
                source_sha256: s.plan.source_sha256,
                pages: s.plan.pages.length,
              }),
            );
            const records = [];
            for (const [version, worker] of [
              ["baseline", baseline],
              ["latest", resolve("target/release/squeeze")],
            ]) {
              const output = join(s.dir, version + ".pdf");
              const { stdout } = await exec(
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
              );
              records.push({ version, ...JSON.parse(stdout) });
            }
            const a = readFileSync(records[0].output),
              b = readFileSync(records[1].output);
            if (b.length > a.length) throw Error("Desktop output size regression");
            let sampleChecks = [];
            if (!a.equals(b)) {
              if (!records[1].image_optimization?.selected) throw Error("Unexplained desktop encoding difference");
              const {stdout} = await exec(process.env.SQUEEZE_QA_PYTHON || "/Users/qq/.cache/codex-runtimes/codex-primary-runtime/dependencies/python/bin/python3",
                [resolve("scripts/verify-raster-samples.py"),records[0].output,records[1].output,s.dir]);
              sampleChecks = JSON.parse(stdout);
            }
            const result = {
              input: s.path,
              preset: s.preset,
              plan: s.plan,
              source_sha256: hash(readFileSync(s.path)),
              rgb_sha256: hash(readFileSync(join(s.dir, "0.rgb"))),
              output_sha256: hash(b),
              byte_equal: a.equals(b),
              sample_checks: sampleChecks,
              records,
            };
            writeFileSync(
              join(s.dir, "result.json"),
              JSON.stringify(result, null, 2),
            );
            res.end(JSON.stringify(records[1]));
          } catch (error) {
            res.statusCode = 500;
            res.end(String(error));
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
  `<!doctype html><meta charset="utf-8"><title>Desktop raster baseline</title><script type="module">
const log=message=>fetch('/__rasterqa/log',{method:'POST',body:JSON.stringify(message)});
window.addEventListener('error',e=>log({error:e.message}));
window.addEventListener('unhandledrejection',e=>log({rejection:String(e.reason)}));
// Offscreen WKWebView throttles rAF; only its scheduling is adapted.
window.requestAnimationFrame=callback=>setTimeout(()=>callback(performance.now()),0);
window.cancelAnimationFrame=clearTimeout;
const {compressRaster}=await import('/src/rasterCompress.ts');
window.__TAURI_INTERNALS__={invoke:async(cmd,args,options)=>{
 const raw=cmd==='write_raster_page';const response=await fetch('/__rasterqa/'+cmd,{method:'POST',body:raw?args:JSON.stringify(args),headers:raw?options.headers:{'Content-Type':'application/json'}});
 if(!response.ok)throw Error(await response.text());return cmd==='read_pdf'?response.arrayBuffer():response.json();
}};
try{
 const files=await (await fetch('/__rasterqa/tests')).json(),results=[];
 const query=new URLSearchParams(location.search);
 for(const path of [files[Number(query.get('case'))]])for(const preset of [query.get('preset')]){
  const report=await compressRaster(path,'qa',preset,message=>log(message));results.push({path,preset,bytes:report.result_bytes});
 }
 window.webkit.messageHandlers.qaResult.postMessage({ok:true,userAgent:navigator.userAgent,devicePixelRatio:devicePixelRatio,results});
}catch(e){window.webkit.messageHandlers.qaResult.postMessage({ok:false,error:String(e),stack:e.stack});}
</script>`,
);
try {
  const cases = [];
  for (let index = 0; index < files.length; index++)
    for (const preset of ["balanced", "small", "lossless"]) {
      const output = join(root, `webkit-${index}-${preset}.json`);
      const url =
        "http://127.0.0.1:1427/" +
        html.slice(resolve(".").length + 1) +
        `?case=${index}&preset=${preset}`;
      const { stderr } = await exec(
        resolve("tmp/round7-streams/run-webkit-qa"),
        [url, output],
        { timeout: 85000 },
      );
      if (stderr) console.error(stderr);
      const result = JSON.parse(readFileSync(output));
      if (!result.ok) throw Error(JSON.stringify(result));
      cases.push(result);
      console.log("Passed", index, preset);
    }
  writeFileSync(
    join(root, "webkit.json"),
    JSON.stringify({ ok: true, isolated_contexts: true, cases }, null, 2),
  );
  console.log(
    cases.length +
      " isolated native WKWebView production-renderer baseline cases passed",
  );
} finally {
  await server.close();
}
