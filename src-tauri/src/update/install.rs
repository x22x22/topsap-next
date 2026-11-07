//! 更新包安装：Windows zip 替换与 macOS dmg 应用包替换。

use std::path::{Path, PathBuf};

/// 准备安装载荷：Windows 解压 zip，macOS 挂载 dmg 并暂存 .app
pub async fn prepare(package: &Path, dir: &Path) -> Result<PathBuf, String> {
    #[cfg(target_os = "windows")]
    {
        prepare_windows(package, dir)
    }
    #[cfg(target_os = "macos")]
    {
        prepare_macos(package, dir).await
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (package, dir);
        Err("当前平台不支持自动更新".to_string())
    }
}

/// 应用安装载荷：启动等待主进程退出的更新助手
pub async fn apply(staged: &Path, package: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let _ = package;
        apply_windows(staged)
    }
    #[cfg(target_os = "macos")]
    {
        let _ = package;
        apply_macos(staged).await
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = (staged, package);
        Err("当前平台不支持自动更新".to_string())
    }
}

/// 启动时清理旧版本备份
pub fn cleanup_old_backup() {
    #[cfg(target_os = "windows")]
    {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let _ = std::fs::remove_dir_all(dir.join("update-backup"));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = current_app_bundle() {
            let _ = std::fs::remove_dir_all(bundle.with_extension("app.old"));
        }
    }
}

#[cfg(target_os = "windows")]
fn prepare_windows(package: &Path, dir: &Path) -> Result<PathBuf, String> {
    let extract = dir.join("extract");
    let _ = std::fs::remove_dir_all(&extract);
    std::fs::create_dir_all(&extract).map_err(|err| format!("创建解压目录失败：{err}"))?;
    extract_zip(package, &extract)?;

    if !extract.join("topsap-client.exe").is_file() {
        return Err("更新包缺少 topsap-client.exe".to_string());
    }
    let dll = if cfg!(target_arch = "aarch64") {
        "wintun-arm64.dll"
    } else {
        "wintun.dll"
    };
    if !extract.join("resources").join(dll).is_file() {
        return Err(format!("更新包缺少 resources/{dll}"));
    }
    Ok(extract)
}

#[cfg(target_os = "windows")]
fn extract_zip(package: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(package).map_err(|err| format!("读取更新包失败：{err}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|err| format!("解压更新包失败：{err}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|err| format!("读取更新包条目失败：{err}"))?;
        let Some(path) = entry.enclosed_name() else {
            return Err("更新包包含非法路径".to_string());
        };
        let out = dest.join(path);
        if entry.is_dir() {
            std::fs::create_dir_all(&out).map_err(|err| format!("创建目录失败：{err}"))?;
            continue;
        }
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|err| format!("创建目录失败：{err}"))?;
        }
        let mut target =
            std::fs::File::create(&out).map_err(|err| format!("写入更新文件失败：{err}"))?;
        std::io::copy(&mut entry, &mut target).map_err(|err| format!("写入更新文件失败：{err}"))?;
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn apply_windows(staged: &Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let exe = std::env::current_exe().map_err(|err| format!("获取程序路径失败：{err}"))?;
    let target = exe.parent().ok_or_else(|| "获取安装目录失败".to_string())?;
    let dir = super::temp_update_dir();
    std::fs::create_dir_all(&dir).map_err(|err| format!("创建临时目录失败：{err}"))?;
    let script = dir.join("apply-update.ps1");
    let log = dir.join("apply-update.log");

    let body = format!(
        "$ErrorActionPreference = 'Stop'\n\
         $parentPid = {pid}\n\
         $target = {target}\n\
         $staging = {staging}\n\
         $backup = Join-Path $target 'update-backup'\n\
         $exe = Join-Path $target 'topsap-client.exe'\n\
         $res = Join-Path $target 'resources'\n\
         $log = {log}\n\
         $tempDir = Split-Path $staging -Parent\n\
         try {{\n\
           while (Get-Process -Id $parentPid -ErrorAction SilentlyContinue) {{ Start-Sleep -Seconds 1 }}\n\
           if (Test-Path $backup) {{ Remove-Item $backup -Recurse -Force }}\n\
           New-Item -ItemType Directory -Path $backup -Force | Out-Null\n\
           Copy-Item $exe (Join-Path $backup 'topsap-client.exe') -Force\n\
           if (Test-Path $res) {{ Copy-Item $res (Join-Path $backup 'resources') -Recurse -Force }}\n\
           Copy-Item (Join-Path $staging 'topsap-client.exe') $exe -Force\n\
           if (Test-Path $res) {{ Remove-Item $res -Recurse -Force }}\n\
           Copy-Item (Join-Path $staging 'resources') $res -Recurse -Force\n\
           Start-Process -FilePath $exe\n\
         }} catch {{\n\
           $_ | Out-File -FilePath $log -Encoding UTF8\n\
           if (Test-Path (Join-Path $backup 'topsap-client.exe')) {{ Copy-Item (Join-Path $backup 'topsap-client.exe') $exe -Force }}\n\
           if (Test-Path (Join-Path $backup 'resources')) {{ if (Test-Path $res) {{ Remove-Item $res -Recurse -Force }}; Copy-Item (Join-Path $backup 'resources') $res -Recurse -Force }}\n\
           Start-Process -FilePath $exe\n\
           exit\n\
         }}\n\
         Remove-Item $staging -Recurse -Force -ErrorAction SilentlyContinue\n\
         Remove-Item $backup -Recurse -Force -ErrorAction SilentlyContinue\n\
         Start-Process -WindowStyle Hidden -FilePath $env:ComSpec -ArgumentList '/c', \"ping 127.0.0.1 -n 3 >nul & rmdir /s /q `\"$tempDir`\"\" -ErrorAction SilentlyContinue\n",
        pid = std::process::id(),
        target = ps_quote(target),
        staging = ps_quote(staged),
        log = ps_quote(&log),
    );

    let mut bytes = vec![0xFF, 0xFE];
    for unit in body.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    std::fs::write(&script, bytes).map_err(|err| format!("写入更新脚本失败：{err}"))?;

    std::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|err| format!("启动更新助手失败：{err}"))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn ps_quote(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "''"))
}

#[cfg(target_os = "macos")]
async fn prepare_macos(package: &Path, dir: &Path) -> Result<PathBuf, String> {
    let mount = dir.join("dmg-mount");
    let _ = std::fs::remove_dir_all(&mount);
    std::fs::create_dir_all(&mount).map_err(|err| format!("创建挂载目录失败：{err}"))?;
    let mount_text = mount.to_string_lossy().to_string();
    let package_text = package.to_string_lossy().to_string();

    let attach = run(
        "hdiutil",
        &[
            "attach",
            "-nobrowse",
            "-readonly",
            "-mountpoint",
            &mount_text,
            &package_text,
        ],
    )?;
    if !attach.status.success() {
        return Err(format!("挂载更新镜像失败：{}", stderr(&attach)));
    }

    let staged = stage_app(&mount, dir);
    let _ = run("hdiutil", &["detach", "-force", &mount_text]);
    let staged = staged?;

    let staged_text = staged.to_string_lossy().to_string();

    let verify = run(
        "codesign",
        &["--verify", "--deep", "--strict", &staged_text],
    )?;
    if !verify.status.success() {
        let sign = run(
            "codesign",
            &["--force", "--deep", "--sign", "-", &staged_text],
        )?;
        if !sign.status.success() {
            return Err(format!("更新包签名校验失败：{}", stderr(&sign)));
        }
        let verify_again = run(
            "codesign",
            &["--verify", "--deep", "--strict", &staged_text],
        )?;
        if !verify_again.status.success() {
            return Err(format!(
                "更新包 ad-hoc 签名后仍无法通过校验：{}",
                stderr(&verify_again)
            ));
        }
    }

    let quarantine = run("xattr", &["-p", "com.apple.quarantine", &staged_text])?;
    if quarantine.status.success() {
        let _ = run("xattr", &["-rd", "com.apple.quarantine", &staged_text]);
    }
    Ok(staged)
}

#[cfg(target_os = "macos")]
fn stage_app(mount: &Path, dir: &Path) -> Result<PathBuf, String> {
    let app = find_app(mount).ok_or_else(|| "更新镜像中未找到应用包".to_string())?;
    std::fs::create_dir_all(dir.join("staged"))
        .map_err(|err| format!("创建暂存目录失败：{err}"))?;
    let name = app
        .file_name()
        .ok_or_else(|| "应用包名称不合法".to_string())?;
    let staged = dir.join("staged").join(name);
    let _ = std::fs::remove_dir_all(&staged);
    let app_text = app.to_string_lossy().to_string();
    let staged_text = staged.to_string_lossy().to_string();
    let copy = run("ditto", &[&app_text, &staged_text])?;
    if !copy.status.success() {
        return Err(format!("复制新应用包失败：{}", stderr(&copy)));
    }
    Ok(staged)
}

#[cfg(target_os = "macos")]
async fn apply_macos(staged: &Path) -> Result<(), String> {
    let bundle = current_app_bundle().ok_or_else(|| "未定位到当前应用包".to_string())?;
    let bundle_text = bundle.to_string_lossy().to_string();
    if bundle_text.contains("/Volumes/") || bundle_text.contains("/AppTranslocation/") {
        return Err("当前应用从镜像或隔离位置运行，请先拖入「应用程序」后再更新".to_string());
    }
    let dir = super::temp_update_dir();
    let helper = dir.join("apply-update.sh");
    let log = dir.join("apply-update.log");
    let user = whoami();
    let uid = id_u();
    let body = format!(
        "#!/bin/sh\n\
         parent_pid={pid}\n\
         target={target}\n\
         staged={staged}\n\
         user={user}\n\
         uid={uid}\n\
         log={log}\n\
         tmpdir={tmpdir}\n\
         open_app() {{\n\
           if [ \"$(id -u)\" = \"0\" ]; then\n\
             launchctl asuser \"$uid\" open \"$target\" || open \"$target\"\n\
           else\n\
             open \"$target\"\n\
           fi\n\
         }}\n\
         while kill -0 \"$parent_pid\" 2>/dev/null; do sleep 1; done\n\
         backup=\"$target.old\"\n\
         rm -rf \"$backup\"\n\
         if ! mv \"$target\" \"$backup\"; then echo \"move old app failed\" > \"$log\"; open_app; exit 1; fi\n\
         if ! ditto \"$staged\" \"$target\"; then\n\
           echo \"copy new app failed\" >> \"$log\"\n\
           rm -rf \"$target\"\n\
           mv \"$backup\" \"$target\"\n\
           open_app\n\
           exit 1\n\
         fi\n\
         xattr -rd com.apple.quarantine \"$target\" 2>/dev/null || true\n\
         if [ \"$(id -u)\" = \"0\" ]; then chown -R \"$user\" \"$target\" 2>/dev/null || true; fi\n\
         if open_app; then\n\
           rm -rf \"$backup\" \"$staged\" \"$tmpdir\"\n\
         else\n\
           echo \"open new app failed\" >> \"$log\"\n\
           rm -rf \"$target\"\n\
           mv \"$backup\" \"$target\"\n\
           open_app\n\
         fi\n",
        pid = std::process::id(),
        target = sh_quote(&bundle_text),
        staged = sh_quote(&staged.to_string_lossy()),
        user = sh_quote(&user),
        uid = uid,
        log = sh_quote(&log.to_string_lossy()),
        tmpdir = sh_quote(&dir.to_string_lossy()),
    );
    std::fs::write(&helper, body).map_err(|err| format!("写入更新脚本失败：{err}"))?;
    set_executable(&helper)?;

    if can_write(bundle.parent().unwrap_or(Path::new("/"))) {
        std::process::Command::new("/bin/sh")
            .arg(&helper)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|err| format!("启动更新助手失败：{err}"))?;
        return Ok(());
    }

    let launcher = dir.join("apply-update-launcher.sh");
    let launcher_body = format!(
        "#!/bin/sh\n/bin/sh {} > {} 2>&1 &\n",
        sh_quote(&helper.to_string_lossy()),
        sh_quote(&log.to_string_lossy()),
    );
    std::fs::write(&launcher, launcher_body).map_err(|err| format!("写入授权脚本失败：{err}"))?;
    let script = format!(
        "do shell script {} with administrator privileges",
        applescript_quote(&format!(
            "/bin/sh {}",
            sh_quote(&launcher.to_string_lossy())
        ))
    );
    let script_path = dir.join("apply-update.applescript");
    std::fs::write(&script_path, script).map_err(|err| format!("写入授权脚本失败：{err}"))?;
    let output = std::process::Command::new("osascript")
        .arg(&script_path)
        .output()
        .map_err(|err| format!("启动管理员授权失败：{err}"))?;
    if !output.status.success() {
        return Err(format!(
            "管理员授权未完成：{}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn find_app(dir: &Path) -> Option<PathBuf> {
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if !file_type.is_dir() {
                continue;
            }
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) == Some("app") {
                return Some(path);
            }
            stack.push(path);
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn current_app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.to_str()?;
    let marker = ".app/Contents/";
    let index = path.find(marker)?;
    Some(PathBuf::from(&path[..index + 4]))
}

#[cfg(target_os = "macos")]
fn can_write(dir: &Path) -> bool {
    let probe = dir.join(".topsap-update-write-test");
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

#[cfg(target_os = "macos")]
fn whoami() -> String {
    std::process::Command::new("id")
        .arg("-un")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "root".to_string()))
}

#[cfg(target_os = "macos")]
fn id_u() -> String {
    std::process::Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "0".to_string())
}

#[cfg(target_os = "macos")]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)
        .map_err(|err| format!("读取脚本权限失败：{err}"))?
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(path, permissions).map_err(|err| format!("设置脚本权限失败：{err}"))
}

#[cfg(target_os = "macos")]
fn sh_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(target_os = "macos")]
fn applescript_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(target_os = "macos")]
fn run(program: &str, args: &[&str]) -> Result<std::process::Output, String> {
    std::process::Command::new(program)
        .args(args)
        .output()
        .map_err(|err| format!("无法启动 {program}：{err}"))
}

#[cfg(target_os = "macos")]
fn stderr(output: &std::process::Output) -> String {
    let text = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if text.is_empty() {
        format!("退出码 {}", output.status.code().unwrap_or(-1))
    } else {
        text
    }
}
