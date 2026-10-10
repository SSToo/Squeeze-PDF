// Real Tauri IPC and production frontend, in one continuous native app context.
// Only the HTML driver/server is QA; import, rendering, staging, cancellation,
// worker execution, quality gate and atomic export are the product commands.
import { createServer } from "vite";
import { spawn } from "node:child_process";
import {
  mkdirSync,
  readFileSync,
  writeFileSync,
  existsSync,
  createWriteStream,
} from "node:fs";
import { resolve, join } from "node:path";
import { createHash } from "node:crypto";
const root = resolve(process.argv[2] || "tmp/round9-remaining/native-ipc");
mkdirSync(root, { recursive: true });
const paths = [
  "tmp/round9-remaining/fixtures/vector-state.pdf",
  "tmp/round9-remaining/fixtures/cid-font-plain.pdf",
  "tmp/round9-remaining/fixtures/alpha-1-dark.pdf",
  "tmp/round8-images/fixtures/shared-10-pages.pdf",
  "tmp/round8-images/fixtures/shared-100-pages.pdf",
].map((p) => resolve(p));
const sha = (p) => createHash("sha256").update(readFileSync(p)).digest("hex");
const sources = paths.map((p) => ({ path: p, sha256: sha(p) }));
const tasks = paths.flatMap((path, i) =>
  (i >= 3 ? ["balanced"] : ["balanced", "small", "lossless"]).map((preset) => ({
    path,
    preset,
    directory: join(root, "exports"),
  })),
);
mkdirSync(join(root, "exports"), { recursive: true });
let doneResolve;
const done = new Promise((resolve) => {
  doneResolve = resolve;
});
const server = await createServer({
  configFile: false,
  root: resolve("."),
  server: {
    host: "127.0.0.1",
    port: 1420,
    strictPort: true,
    hmr: false,
    watch: { ignored: ["**/tmp/**", "**/target/**"] },
  },
  plugins: [
    {
      name: "native-ipc-qa",
      configureServer(server) {
        server.middlewares.use(async (req, res, next) => {
          if (!req.url.startsWith("/__nativeqa/")) return next();
          let b = "";
          for await (const c of req) b += c;
          if (req.url === "/__nativeqa/tasks") {
            res.end(JSON.stringify({ tasks, root }));
            return;
          }
          if (req.url === "/__nativeqa/log") {
            console.log("NATIVE", b);
            res.end("null");
            return;
          }
          if (req.url === "/__nativeqa/result") {
            const result = JSON.parse(b);
            for (const s of sources)
              if (sha(s.path) !== s.sha256) throw Error("Source changed");
            writeFileSync(
              join(root, "result.json"),
              JSON.stringify({ ...result, sources }, null, 2),
            );
            res.end("null");
            doneResolve(result);
            return;
          }
          res.statusCode = 404;
          res.end();
        });
      },
    },
  ],
});
const html = join(root, "native-qa.html");
writeFileSync(
  html,
  `<!doctype html><meta charset="utf-8"><title>Squeeze PDF · Native IPC QA</title><p id="status">正在验证本地压缩与取消流程…</p><script type="module">
import {invoke} from '/node_modules/@tauri-apps/api/core.js';
import {compressRaster,cancelRaster} from '/src/rasterCompress.ts';
const log=message=>fetch('/__nativeqa/log',{method:'POST',body:JSON.stringify(message)});
window.addEventListener('error',e=>log({error:e.message}));window.addEventListener('unhandledrejection',e=>log({rejection:String(e.reason)}));
let cancelPromise;const operations=[];
const results=[];
try{
 const {tasks,root}=await(await fetch('/__nativeqa/tasks')).json();
 const imported=await invoke('import_files',{paths:[...new Set(tasks.map(t=>t.path))]});if(imported.some(i=>i.error))throw Error(JSON.stringify(imported));
 // Invalid offset is rejected by real Rust IPC, then the next session can start.
 const s=await invoke('begin_raster',tasks[0]);let rejected=false;try{await invoke('write_raster_page',new Uint8Array([1,2,3]),{headers:{'x-squeeze-session':s.id,'x-squeeze-page':'0','x-squeeze-offset':'1','x-squeeze-variant':'direct'}});}catch(e){rejected=true;}if(!rejected)throw Error('Invalid offset accepted');await invoke('abort_raster',{id:s.id});
 let cancelled=false;let scheduled=false;try{await compressRaster(tasks[1].path,tasks[1].directory,'small',message=>{log(message);if(!scheduled){scheduled=true;setTimeout(()=>{cancelRaster();cancelPromise=invoke('cancel_task',{});},1);}});}catch(e){cancelled=String(e).includes('取消');}await cancelPromise;if(!cancelled)throw Error('Cancellation did not reject');
 for(const t of tasks){document.getElementById('status').textContent=t.path.split('/').at(-1)+' · '+t.preset;const r=await compressRaster(t.path,t.directory,t.preset,log);results.push({input:t.path,preset:t.preset,...r});}
 if(results.some(r=>r.desktop_rendering.max_chunk_bytes>1024*1024))throw Error('Chunk budget exceeded');
 await fetch('/__nativeqa/result',{method:'POST',body:JSON.stringify({ok:true,invalid_chunk_rejected:rejected,cancelled_then_retried:cancelled,operations,results,userAgent:navigator.userAgent,devicePixelRatio})});
}catch(e){await fetch('/__nativeqa/result',{method:'POST',body:JSON.stringify({ok:false,error:String(e),stack:e.stack,results,operations})});}
</script>`,
);
const config = join(root, "tauri-qa.json");
writeFileSync(
  config,
  JSON.stringify({
    build: {
      beforeDevCommand: "",
      devUrl: "http://127.0.0.1:1420/" + html.slice(resolve(".").length + 1),
    },
  }),
);
await server.listen();
const output = createWriteStream(join(root, "app.log"));
const child = spawn(
  resolve("node_modules/.bin/tauri"),
  ["dev", "--no-watch", "--no-dev-server", "--config", config],
  { cwd: resolve("."), detached: true, stdio: ["ignore", "pipe", "pipe"] },
);
child.stdout.pipe(output);
child.stderr.pipe(output);
let timeout;
try {
  const result = await Promise.race([
    done,
    new Promise((_, reject) => {
      timeout = setTimeout(
        () => reject(Error("Native IPC QA timeout")),
        300000,
      );
      child.on("exit", (code) => {
        if (!existsSync(join(root, "result.json")))
          reject(Error("Native app exited " + code));
      });
    }),
  ]);
  if (!result.ok) throw Error(JSON.stringify(result));
  console.log(
    result.results.length +
      " real Tauri IPC tasks passed; cancel/retry and invalid chunks verified",
  );
} finally {
  clearTimeout(timeout);
  try {
    process.kill(-child.pid, "SIGTERM");
  } catch {}
  await server.close();
  output.end();
}
