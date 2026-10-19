use tauri::{plugin::TauriPlugin, AppHandle, Manager, Runtime};
use tauri_plugin_log::{log, Builder, Target, TargetKind};

type Logger = (log::LevelFilter, Box<dyn log::Log>);

/// 输出端初始化仍在 Builder 的插件阶段完成，不在应用 setup 中动态注册插件，
/// 避免改变 Android 首屏加载时日志命令的可用时机。
pub fn init<R: Runtime>() -> TauriPlugin<R> {
    tauri::plugin::Builder::<R>::new("app-logging")
        .setup(|app, _| {
            initialize(app);
            Ok(())
        })
        .build()
}

fn initialize<R: Runtime>(app: &AppHandle<R>) {
    let (logger, file_error) =
        with_file_fallback(|| acquire_logger(app, true), || acquire_logger(app, false));
    let warning = file_error.map(|error| {
        let path = app
            .path()
            .app_log_dir()
            .map(|directory| {
                directory
                    .join(&app.package_info().name)
                    .with_extension("log")
                    .display()
                    .to_string()
            })
            .unwrap_or_else(|path_error| format!("无法确定日志路径: {path_error}"));
        format!(
            "[startup] 无法初始化文件日志 path={path}: {error}；本次仅输出到控制台和 Webview，不修改原日志或权限"
        )
    });

    // 在全局 logger 尚不可用时也必须给出具体的失败路径与原因。
    if let Some(warning) = &warning {
        eprintln!("{warning}");
    }
    match logger {
        Ok((level, logger)) => match tauri_plugin_log::attach_logger(level, logger) {
            Ok(()) => {
                if let Some(warning) = warning {
                    log::warn!("{warning}");
                }
            }
            Err(error) => eprintln!("[startup] 无法注册应用日志，保留已有 logger: {error}"),
        },
        Err(error) => eprintln!("[startup] 控制台日志也无法初始化，继续启动应用: {error}"),
    }
}

fn acquire_logger<R: Runtime>(
    app: &AppHandle<R>,
    include_file: bool,
) -> Result<Logger, tauri_plugin_log::Error> {
    let mut targets = vec![Target::new(TargetKind::Stdout)];
    if include_file {
        targets.push(Target::new(TargetKind::LogDir { file_name: None }));
    }
    targets.push(Target::new(TargetKind::Webview));
    Builder::new()
        // 与原配置一致：业务日志保留 INFO，网络/TLS 依赖仅保留警告。
        .level(log::LevelFilter::Info)
        .level_for("rustls", log::LevelFilter::Warn)
        .level_for("reqwest", log::LevelFilter::Warn)
        .level_for("s3", log::LevelFilter::Warn)
        .targets(targets)
        .split(app)
        .map(|(_, level, logger)| (level, logger))
}

fn with_file_fallback<T, E>(
    with_file: impl FnOnce() -> Result<T, E>,
    without_file: impl FnOnce() -> Result<T, E>,
) -> (Result<T, E>, Option<E>) {
    match with_file() {
        Ok(logger) => (Ok(logger), None),
        Err(error) => (without_file(), Some(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::with_file_fallback;
    use std::{fs, io};

    #[test]
    fn writable_file_keeps_normal_logging_without_constructing_a_fallback() {
        let (result, file_error) = with_file_fallback(
            || Ok::<_, io::Error>("file, stdout and webview"),
            || panic!("文件日志成功时不应再次初始化 logger"),
        );
        assert_eq!(result.unwrap(), "file, stdout and webview");
        assert!(file_error.is_none());
    }

    #[test]
    fn denied_file_uses_console_logging_and_preserves_the_original_error() {
        let (result, file_error) = with_file_fallback(
            || Err(io::Error::from(io::ErrorKind::PermissionDenied)),
            || Ok("stdout and webview"),
        );
        assert_eq!(result.unwrap(), "stdout and webview");
        assert_eq!(file_error.unwrap().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn fallback_failure_is_reported_without_panicking_or_losing_the_file_error() {
        let (result, file_error) = with_file_fallback::<(), _>(
            || Err(io::Error::from(io::ErrorKind::PermissionDenied)),
            || Err(io::Error::from(io::ErrorKind::Other)),
        );
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::Other);
        assert_eq!(file_error.unwrap().kind(), io::ErrorKind::PermissionDenied);
    }

    #[test]
    fn file_initialization_failure_never_removes_or_rewrites_the_existing_path() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("application.log");
        fs::create_dir(&path).unwrap();
        let marker = path.join("must-not-delete");
        fs::write(&marker, b"original data").unwrap();
        let (result, file_error) = with_file_fallback(
            || {
                fs::OpenOptions::new()
                    .append(true)
                    .create(true)
                    .open(&path)
                    .map(Some)
            },
            || Ok(None),
        );
        assert!(result.unwrap().is_none());
        assert!(file_error.is_some());
        assert_eq!(fs::read(&marker).unwrap(), b"original data");
    }
}
