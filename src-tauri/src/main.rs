#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tauri::{
    Emitter, Manager, State,
    ipc::{InvokeBody, Request, Response},
};
use tauri_plugin_opener::OpenerExt;
use tauri_plugin_shell::{
    ShellExt,
    process::{CommandChild, CommandEvent},
};

#[derive(Default)]
struct EngineState {
    allowed: Mutex<HashSet<PathBuf>>,
    child: Mutex<Option<CommandChild>>,
    busy: AtomicBool,
    cancelled: AtomicBool,
    raster: Mutex<Option<RasterSession>>,
}
struct RasterSession {
    id: String,
    input: PathBuf,
    output: PathBuf,
    preset: squeeze_core::Preset,
    plan: squeeze_core::RasterPlan,
    directory: tempfile::TempDir,
    received: usize,
    started: std::time::Instant,
}
#[derive(Serialize)]
struct RasterStart {
    id: String,
    plan: squeeze_core::RasterPlan,
}
#[derive(Serialize)]
struct Imported {
    path: PathBuf,
    name: String,
    info: Option<squeeze_core::DocumentInfo>,
    error: Option<String>,
}
#[tauri::command]
async fn import_files(
    paths: Vec<String>,
    state: State<'_, Arc<EngineState>>,
) -> Result<Vec<Imported>, String> {
    if paths.len() > 100 {
        return Err("一次最多添加 100 个文件".into());
    }
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|p| {
                let path = std::fs::canonicalize(&p).map_err(|e| e.to_string())?;
                if !path
                    .extension()
                    .is_some_and(|s| s.eq_ignore_ascii_case("pdf"))
                {
                    return Err("请选择 PDF 文件".into());
                }
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                let result = squeeze_core::inspect_file(&path, &squeeze_core::Limits::default());
                state.allowed.lock().unwrap().insert(path.clone());
                Ok(match result {
                    Ok(info) => Imported {
                        path,
                        name,
                        info: Some(info),
                        error: None,
                    },
                    Err(error) => Imported {
                        path,
                        name,
                        info: None,
                        error: Some(error.to_string()),
                    },
                })
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())?
}
fn authorize(state: &EngineState, path: &str) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(path).map_err(|e| e.to_string())?;
    if !state.allowed.lock().unwrap().contains(&canonical) {
        return Err("请先添加这个文件".into());
    }
    Ok(canonical)
}
fn unique_output(input: &Path, directory: &Path) -> Result<PathBuf, String> {
    if !directory.is_dir() {
        return Err("输出目录不存在".into());
    }
    let stem = input.file_stem().unwrap_or_default().to_string_lossy();
    for n in 0..10000 {
        let suffix = if n == 0 {
            String::new()
        } else {
            format!("_{n}")
        };
        let candidate = directory.join(format!("{stem}_compressed{suffix}.pdf"));
        if !candidate.try_exists().map_err(|e| e.to_string())? {
            return Ok(candidate);
        }
    }
    Err("同名文件太多，请选择其他目录".into())
}
#[tauri::command]
async fn compress_file(
    app: tauri::AppHandle,
    state: State<'_, Arc<EngineState>>,
    path: String,
    directory: String,
    preset: squeeze_core::Preset,
    output_mode: squeeze_core::OutputMode,
) -> Result<Value, String> {
    let state = state.inner().clone();
    let input = authorize(&state, &path)?;
    let output = unique_output(&input, Path::new(&directory))?;
    if state
        .busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("已有压缩任务在运行".into());
    }
    state.cancelled.store(false, Ordering::SeqCst);
    let result = run_worker(&app, &state, &input, &output, preset, output_mode, None).await;
    state.busy.store(false, Ordering::SeqCst);
    if let Ok(report) = &result
        && let Some(path) = report.get("output").and_then(Value::as_str)
    {
        state
            .allowed
            .lock()
            .unwrap()
            .insert(std::fs::canonicalize(path).map_err(|e| e.to_string())?);
    }
    result
}
async fn run_worker(
    app: &tauri::AppHandle,
    state: &EngineState,
    input: &Path,
    output: &Path,
    preset: squeeze_core::Preset,
    output_mode: squeeze_core::OutputMode,
    raster_manifest: Option<&Path>,
) -> Result<Value, String> {
    let temporary =
        tempfile::tempdir_in(output.parent().ok_or("输出目录无效")?).map_err(|e| e.to_string())?;
    let mode = match preset {
        squeeze_core::Preset::Lossless if output_mode == squeeze_core::OutputMode::Preserve => {
            "fidelity"
        }
        squeeze_core::Preset::Lossless => "lossless",
        squeeze_core::Preset::Fidelity => "fidelity",
        squeeze_core::Preset::Balanced => "balanced",
        squeeze_core::Preset::Small => "small",
    };
    let mut command = app
        .shell()
        .sidecar("squeeze-worker")
        .map_err(|e| e.to_string())?
        .args([
            "compress",
            input.to_str().ok_or("文件路径无法编码")?,
            "--output",
            output.to_str().ok_or("输出路径无法编码")?,
            "--preset",
            mode,
            "--output-mode",
            match output_mode {
                squeeze_core::OutputMode::Preserve => "preserve",
                squeeze_core::OutputMode::Raster => "raster",
            },
            "--json",
            "--events",
            "--temp-dir",
            temporary.path().to_str().ok_or("临时路径无法编码")?,
        ]);
    if let Some(manifest) = raster_manifest {
        command = command.args([
            "--raster-manifest",
            manifest.to_str().ok_or("像素清单路径无法编码")?,
        ]);
    }
    if output_mode == squeeze_core::OutputMode::Preserve {
        command = command.args(["--compact-paths", "--reuse-paths"]);
    }
    let (mut events, child) = command.spawn().map_err(|e| e.to_string())?;
    *state.child.lock().unwrap() = Some(child);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(120);
    let mut stdout = Vec::new();
    let mut error = String::new();
    let mut terminated = false;
    let result: Result<Value, String> = async {
        loop {
            let event = tokio::time::timeout_at(deadline, events.recv())
                .await
                .map_err(|_| "处理超时，已终止任务")?
                .ok_or("引擎意外停止")?;
            match event {
                CommandEvent::Stdout(bytes) => {
                    if stdout.len() + bytes.len() > 16 * 1024 * 1024 {
                        return Err("引擎报告超过限制".into());
                    }
                    stdout.extend(bytes);
                }
                CommandEvent::Stderr(bytes) => {
                    if let Ok(progress) = serde_json::from_slice::<Value>(&bytes) {
                        let _ = app.emit(
                            "squeeze-progress",
                            json!({"path":input,"progress":progress}),
                        );
                    } else if error.len() < 8192 {
                        error.push_str(&String::from_utf8_lossy(&bytes));
                    }
                }
                CommandEvent::Error(message) => return Err(message),
                CommandEvent::Terminated(payload) => {
                    terminated = true;
                    // Do not delete a committed output if cancellation arrives after
                    // the engine completed its atomic export.
                    if payload.code == Some(0) {
                        return serde_json::from_slice(&stdout)
                            .map_err(|e| format!("引擎报告无法读取：{e}"));
                    }
                    if state.cancelled.load(Ordering::SeqCst) {
                        return Err("任务已取消".into());
                    }
                    return Err(if error.is_empty() {
                        "引擎执行失败".into()
                    } else {
                        error
                    });
                }
                _ => {}
            }
        }
    }
    .await;
    if let Some(child) = state.child.lock().unwrap().take()
        && !terminated
    {
        let _ = child.kill();
    }
    if !terminated {
        // Wait for the process to release file handles before deleting its
        // staging directory. This also matters on Windows after force-kill.
        let drain = async {
            while let Some(event) = events.recv().await {
                if matches!(event, CommandEvent::Terminated(_)) {
                    break;
                }
            }
        };
        if tokio::time::timeout(std::time::Duration::from_secs(5), drain)
            .await
            .is_err()
        {
            let _ = temporary.keep();
            return Err("引擎未及时退出，临时目录保留以避免删除正在使用的文件".into());
        }
    }
    temporary
        .close()
        .map_err(|e| format!("清理临时文件失败：{e}"))?;
    result
}
#[tauri::command]
fn cancel_task(state: State<'_, Arc<EngineState>>) -> Result<(), String> {
    state.cancelled.store(true, Ordering::SeqCst);
    if let Some(child) = state.child.lock().unwrap().take() {
        child.kill().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
async fn begin_raster(
    state: State<'_, Arc<EngineState>>,
    path: String,
    directory: String,
    preset: squeeze_core::Preset,
) -> Result<RasterStart, String> {
    let state = state.inner().clone();
    let input = authorize(&state, &path)?;
    let output = unique_output(&input, Path::new(&directory))?;
    if state
        .busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("已有压缩任务在运行".into());
    }
    state.cancelled.store(false, Ordering::SeqCst);
    let owner = state.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let options = squeeze_core::Options {
            preset,
            output_mode: squeeze_core::OutputMode::Raster,
            ..Default::default()
        };
        let plan = squeeze_core::prepare_raster(&input, &options, &owner.cancelled)
            .map_err(|e| e.to_string())?;
        if owner.cancelled.load(Ordering::SeqCst) {
            return Err("任务已取消".into());
        }
        let directory = tempfile::tempdir_in(output.parent().ok_or("输出目录无效")?)
            .map_err(|e| e.to_string())?;
        let id = directory
            .path()
            .file_name()
            .ok_or("临时目录无效")?
            .to_string_lossy()
            .into_owned();
        let start = RasterStart {
            id: id.clone(),
            plan: plan.clone(),
        };
        *owner.raster.lock().unwrap() = Some(RasterSession {
            id,
            input,
            output,
            preset,
            plan,
            directory,
            received: 0,
            started: std::time::Instant::now(),
        });
        Ok(start)
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|v| v);
    if result.is_err() {
        state.busy.store(false, Ordering::SeqCst);
    }
    result
}

#[tauri::command]
fn write_raster_page(
    state: State<'_, Arc<EngineState>>,
    request: Request<'_>,
) -> Result<(), String> {
    if state.cancelled.load(Ordering::SeqCst) {
        return Err("任务已取消".into());
    }
    let id = request
        .headers()
        .get("x-squeeze-session")
        .and_then(|s| s.to_str().ok())
        .ok_or("缺少页面会话")?;
    let index: usize = request
        .headers()
        .get("x-squeeze-page")
        .and_then(|s| s.to_str().ok())
        .and_then(|s| s.parse().ok())
        .ok_or("页面编号无效")?;
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err("页面像素必须使用二进制传输".into());
    };
    let mut locked = state.raster.lock().unwrap();
    let session = locked
        .as_mut()
        .filter(|s| s.id == id)
        .ok_or("页面会话已结束")?;
    if session.started.elapsed().as_secs() > 120 {
        return Err("页面渲染超时".into());
    }
    if index != session.received {
        return Err("页面像素顺序无效".into());
    }
    let page = session
        .plan
        .pages
        .get(index)
        .ok_or("页面编号超过原件范围")?;
    let expected = page.dimensions[0] as usize * page.dimensions[1] as usize * 3;
    if bytes.len() != expected {
        return Err("页面像素长度与尺寸不符".into());
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(session.directory.path().join(format!("{index}.rgb")))
        .map_err(|e| e.to_string())?;
    file.write_all(bytes).map_err(|e| e.to_string())?;
    session.received += 1;
    Ok(())
}

#[tauri::command]
fn abort_raster(state: State<'_, Arc<EngineState>>, id: String) -> Result<(), String> {
    let mut locked = state.raster.lock().unwrap();
    if locked.as_ref().is_some_and(|s| s.id == id) {
        locked.take();
        state.busy.store(false, Ordering::SeqCst);
    }
    Ok(())
}

#[tauri::command]
async fn finish_raster(
    app: tauri::AppHandle,
    state: State<'_, Arc<EngineState>>,
    id: String,
) -> Result<Value, String> {
    let state = state.inner().clone();
    let session = {
        let mut locked = state.raster.lock().unwrap();
        let s = locked
            .as_ref()
            .filter(|s| s.id == id)
            .ok_or("页面会话已结束")?;
        if s.received != s.plan.pages.len() {
            return Err("页面渲染尚未完成".into());
        }
        locked.take().unwrap()
    };
    let result: Result<Value, String> = async {
        if state.cancelled.load(Ordering::SeqCst) {
            return Err("任务已取消".into());
        }
        let manifest = session.directory.path().join("manifest.json");
        let bytes = serde_json::to_vec(&squeeze_core::RasterManifest {
            source_sha256: session.plan.source_sha256.clone(),
            pages: session.plan.pages.len(),
        })
        .map_err(|e| e.to_string())?;
        std::fs::write(&manifest, bytes).map_err(|e| e.to_string())?;
        let report = run_worker(
            &app,
            &state,
            &session.input,
            &session.output,
            session.preset,
            squeeze_core::OutputMode::Raster,
            Some(&manifest),
        )
        .await?;
        if let Some(path) = report.get("output").and_then(Value::as_str) {
            state
                .allowed
                .lock()
                .unwrap()
                .insert(std::fs::canonicalize(path).map_err(|e| e.to_string())?);
        }
        Ok(report)
    }
    .await;
    // The worker has terminated before its input pixel directory is dropped.
    if result.as_ref().is_err_and(|e| e.contains("引擎未及时退出")) {
        let _ = session.directory.keep();
    }
    state.busy.store(false, Ordering::SeqCst);
    result
}
#[tauri::command]
async fn read_pdf(path: String, state: State<'_, Arc<EngineState>>) -> Result<Response, String> {
    let path = authorize(&state, &path)?;
    tauri::async_runtime::spawn_blocking(move || {
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        Ok(Response::new(bytes))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
fn open_result(
    app: tauri::AppHandle,
    state: State<'_, Arc<EngineState>>,
    path: String,
    folder: bool,
) -> Result<(), String> {
    let path = authorize(&state, &path)?;
    if folder {
        app.opener()
            .reveal_item_in_dir(path)
            .map_err(|e| e.to_string())
    } else {
        app.opener()
            .open_path(path.to_str().ok_or("文件路径无法编码")?, None::<&str>)
            .map_err(|e| e.to_string())
    }
}
fn main() {
    tauri::Builder::default()
        .manage(Arc::new(EngineState::default()))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            import_files,
            compress_file,
            cancel_task,
            read_pdf,
            open_result,
            begin_raster,
            write_raster_page,
            abort_raster,
            finish_raster
        ])
        .build(tauri::generate_context!())
        .expect("应用初始化失败")
        .run(|app, event| {
            if matches!(event, tauri::RunEvent::Exit) {
                let state = app.state::<Arc<EngineState>>();
                if let Some(child) = state.child.lock().unwrap().take() {
                    let _ = child.kill();
                }
                state.raster.lock().unwrap().take();
            }
        });
}
